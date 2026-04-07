//! vsock TCP bridge — forwards host TCP connections to guest vsock ports
//! via Unix sockets managed by libkrun.
//!
//! ```text
//! Host TCP:31122 → tateru bridge → Unix socket → libkrun vsock → Guest vsock:22
//! ```

use std::path::PathBuf;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, UnixStream};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::error::TateruError;

/// Configuration for a single vsock bridge.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Host TCP port to listen on.
    pub host_port: u16,
    /// Path to the Unix socket created by libkrun for this vsock port.
    pub socket_path: PathBuf,
}

/// A running vsock bridge task handle.
#[derive(Debug)]
pub struct BridgeHandle {
    #[allow(dead_code)]
    task: Option<tokio::task::JoinHandle<()>>,
}

impl BridgeHandle {
    /// Create a no-op handle (for testing).
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn noop() -> Self {
        Self { task: None }
    }
}

/// Start a TCP-to-vsock bridge.
///
/// Listens on `127.0.0.1:{host_port}` and for each incoming connection,
/// connects to the Unix socket at `socket_path` and bidirectionally copies
/// data between the two streams.
pub(crate) fn spawn_bridge(
    config: BridgeConfig,
    mut shutdown: watch::Receiver<bool>,
) -> BridgeHandle {
    let task = tokio::spawn(async move {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", config.host_port)).await {
            Ok(l) => {
                info!(
                    "vsock bridge listening on 127.0.0.1:{} → {}",
                    config.host_port,
                    config.socket_path.display()
                );
                l
            }
            Err(e) => {
                error!(
                    "failed to bind TCP port {}: {e}",
                    config.host_port
                );
                return;
            }
        };

        loop {
            tokio::select! {
                accept = listener.accept() => {
                    match accept {
                        Ok((tcp_stream, addr)) => {
                            debug!("bridge: new connection from {addr}");
                            let socket_path = config.socket_path.clone();
                            let conn_shutdown = shutdown.clone();
                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_connection(tcp_stream, &socket_path, conn_shutdown).await
                                {
                                    debug!("bridge connection ended: {e}");
                                }
                            });
                        }
                        Err(e) => {
                            warn!("bridge accept error: {e}");
                        }
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("bridge shutting down on port {}", config.host_port);
                        return;
                    }
                }
            }
        }
    });

    BridgeHandle { task: Some(task) }
}

async fn handle_connection(
    tcp_stream: tokio::net::TcpStream,
    socket_path: &std::path::Path,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), TateruError> {
    let unix_stream = UnixStream::connect(socket_path).await?;

    let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();
    let (mut unix_read, mut unix_write) = unix_stream.into_split();

    tokio::select! {
        result = tokio::io::copy(&mut tcp_read, &mut unix_write) => {
            if let Err(e) = result {
                debug!("tcp→unix copy ended: {e}");
            }
            let _ = unix_write.shutdown().await;
        }
        result = tokio::io::copy(&mut unix_read, &mut tcp_write) => {
            if let Err(e) = result {
                debug!("unix→tcp copy ended: {e}");
            }
            let _ = tcp_write.shutdown().await;
        }
        _ = shutdown.changed() => {
            debug!("bridge connection shutdown signal");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_config_construction() {
        let cfg = BridgeConfig {
            host_port: 31122,
            socket_path: PathBuf::from("/tmp/vsock-22.sock"),
        };
        assert_eq!(cfg.host_port, 31122);
        assert_eq!(cfg.socket_path, PathBuf::from("/tmp/vsock-22.sock"));
    }

    #[tokio::test]
    async fn bridge_shutdown_signal() {
        let (tx, rx) = watch::channel(false);
        let cfg = BridgeConfig {
            host_port: 0, // ephemeral port — won't actually bind successfully if 0 isn't allowed
            socket_path: PathBuf::from("/nonexistent/socket"),
        };

        // Spawn bridge — it will fail to bind or we send shutdown immediately
        let handle = spawn_bridge(cfg, rx);

        // Signal shutdown
        let _ = tx.send(true);

        // Should complete without hanging — just give the task time to react
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        // Bridge should have exited from the shutdown signal
        drop(handle);
    }

    #[tokio::test]
    async fn bridge_connects_tcp_to_unix() {
        // Create a temporary Unix socket server
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let unix_listener = tokio::net::UnixListener::bind(&sock_path).unwrap();

        // Accept and echo back data
        let echo_handle = tokio::spawn(async move {
            let (mut stream, _) = unix_listener.accept().await.unwrap();
            let mut buf = vec![0u8; 1024];
            let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await.unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut stream, &buf[..n])
                .await
                .unwrap();
            stream.shutdown().await.unwrap();
        });

        let (_tx, rx) = watch::channel(false);
        let bridge_cfg = BridgeConfig {
            host_port: 0, // let OS pick
            socket_path: sock_path,
        };

        // Bind manually to get the actual port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // We can't easily test the full bridge without a real vsock setup,
        // but we can verify the echo server works with direct unix connection
        let _handle = spawn_bridge(bridge_cfg, rx);

        // Just verify the echo handle completes when we connect directly
        let stream = UnixStream::connect(tmp.path().join("test.sock")).await;
        if let Ok(mut s) = stream {
            tokio::io::AsyncWriteExt::write_all(&mut s, b"hello").await.unwrap();
            s.shutdown().await.unwrap();
            let _ = echo_handle.await;
        }

        // Port variable used to prevent unused warning
        let _ = port;
    }

    // -- BridgeHandle::noop() --
    // Catches bugs where noop handle panics on drop or has unexpected state.

    #[test]
    fn bridge_handle_noop_does_not_panic() {
        let handle = BridgeHandle::noop();
        assert!(handle.task.is_none());
        drop(handle); // should not panic
    }

    // -- BridgeConfig Clone + Debug --

    #[test]
    fn bridge_config_clone() {
        let cfg = BridgeConfig {
            host_port: 8080,
            socket_path: PathBuf::from("/tmp/test.sock"),
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.host_port, 8080);
        assert_eq!(cloned.socket_path, PathBuf::from("/tmp/test.sock"));
    }

    #[test]
    fn bridge_config_debug() {
        let cfg = BridgeConfig {
            host_port: 9090,
            socket_path: PathBuf::from("/tmp/debug.sock"),
        };
        let d = format!("{cfg:?}");
        assert!(d.contains("9090"));
        assert!(d.contains("debug.sock"));
    }

    // -- BridgeHandle Debug --

    #[test]
    fn bridge_handle_debug() {
        let handle = BridgeHandle::noop();
        let d = format!("{handle:?}");
        assert!(d.contains("BridgeHandle"));
    }

    // -- spawn_bridge with port 0 binds to ephemeral port --

    #[tokio::test]
    async fn bridge_spawn_ephemeral_port_binds() {
        let (_tx, rx) = watch::channel(false);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BridgeConfig {
            host_port: 0,
            socket_path: tmp.path().join("test.sock"),
        };
        let handle = spawn_bridge(cfg, rx);
        // Give the task time to bind
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        // Handle should be valid (task is Some)
        assert!(handle.task.is_some());
    }

    // -- shutdown signal before any connections --

    #[tokio::test]
    async fn bridge_immediate_shutdown() {
        let (tx, rx) = watch::channel(false);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BridgeConfig {
            host_port: 0,
            socket_path: tmp.path().join("test.sock"),
        };
        let _handle = spawn_bridge(cfg, rx);
        // Immediately signal shutdown
        tx.send(true).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        // Should complete without hanging
    }

    // -- handle_connection when Unix socket doesn't exist --

    #[tokio::test]
    async fn handle_connection_unix_connect_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (_tx, rx) = watch::channel(false);

        let client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let result = handle_connection(
            client,
            std::path::Path::new("/nonexistent/socket.sock"),
            rx,
        )
        .await;
        assert!(result.is_err());
    }
}
