//! Shutdown handle wrapping the libkrun eventfd.
//!
//! The [`Shutdown`] trait abstracts shutdown signaling so tests can mock it
//! without requiring real file descriptors.

use std::os::fd::FromRawFd;
use std::os::fd::OwnedFd;

use crate::error::TateruError;

/// Abstraction for triggering VM shutdown.
///
/// Real implementation writes to the libkrun eventfd. Tests can substitute
/// a mock that records calls without requiring real file descriptors.
pub trait Shutdown: Send + std::fmt::Debug {
    /// Signal the VM to shut down.
    fn trigger(&self) -> Result<(), TateruError>;
}

/// Handle to trigger VM shutdown via the libkrun eventfd.
///
/// Writing to the eventfd signals the guest to shut down orderly.
pub struct EventfdShutdown {
    fd: OwnedFd,
}

impl EventfdShutdown {
    /// Create from a raw eventfd file descriptor returned by `krun_get_shutdown_eventfd`.
    ///
    /// # Safety
    ///
    /// The caller must ensure `raw_fd` is a valid, open file descriptor
    /// that was returned by `krun_get_shutdown_eventfd`.
    pub(crate) unsafe fn from_raw_fd(raw_fd: i32) -> Self {
        Self {
            fd: unsafe { OwnedFd::from_raw_fd(raw_fd) },
        }
    }
}

impl Shutdown for EventfdShutdown {
    fn trigger(&self) -> Result<(), TateruError> {
        use std::os::fd::AsRawFd;

        let val: u64 = 1;
        let ret = unsafe {
            libc::write(
                self.fd.as_raw_fd(),
                std::ptr::from_ref(&val).cast(),
                std::mem::size_of::<u64>(),
            )
        };
        if ret < 0 {
            Err(TateruError::BridgeIo(std::io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }
}

impl std::fmt::Debug for EventfdShutdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::os::fd::AsRawFd;
        f.debug_struct("EventfdShutdown")
            .field("fd", &self.fd.as_raw_fd())
            .finish()
    }
}

/// Mock shutdown handle for testing.
///
/// Records whether `trigger()` was called. Never touches real file descriptors.
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Default)]
pub struct MockShutdown {
    /// Number of times `trigger()` has been called.
    pub trigger_count: std::sync::atomic::AtomicU32,
    /// If true, `trigger()` returns an error.
    pub fail: std::sync::atomic::AtomicBool,
}

#[cfg(any(test, feature = "testing"))]
impl MockShutdown {
    /// Create a new mock shutdown handle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many times `trigger()` was called.
    #[must_use]
    pub fn triggered(&self) -> u32 {
        self.trigger_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "testing"))]
impl Shutdown for MockShutdown {
    fn trigger(&self) -> Result<(), TateruError> {
        self.trigger_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(TateruError::BridgeIo(std::io::Error::new(
                std::io::ErrorKind::Other,
                "mock shutdown failure",
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_shutdown_trigger() {
        let shutdown = MockShutdown::new();
        assert_eq!(shutdown.triggered(), 0);
        shutdown.trigger().unwrap();
        assert_eq!(shutdown.triggered(), 1);
        shutdown.trigger().unwrap();
        assert_eq!(shutdown.triggered(), 2);
    }

    #[test]
    fn mock_shutdown_fail() {
        let shutdown = MockShutdown::new();
        shutdown
            .fail
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let err = shutdown.trigger().unwrap_err();
        assert!(matches!(err, TateruError::BridgeIo(_)));
        // Still records the call even on failure
        assert_eq!(shutdown.triggered(), 1);
    }

    #[test]
    fn mock_shutdown_debug() {
        let shutdown = MockShutdown::new();
        let debug = format!("{shutdown:?}");
        assert!(debug.contains("MockShutdown"));
    }

    #[test]
    fn eventfd_shutdown_debug() {
        let mut fds = [0i32; 2];
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(ret, 0);
        unsafe { libc::close(fds[0]) };
        let shutdown = unsafe { EventfdShutdown::from_raw_fd(fds[1]) };
        let debug = format!("{shutdown:?}");
        assert!(debug.contains("EventfdShutdown"));
    }

    // Verify EventfdShutdown::trigger() actually writes to the pipe.
    // Catches bugs where the write call is malformed, the wrong size is used,
    // or cast issues corrupt the fd.
    #[test]
    fn eventfd_shutdown_trigger_writes() {
        let mut fds = [0i32; 2];
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(ret, 0);
        let shutdown = unsafe { EventfdShutdown::from_raw_fd(fds[1]) };
        shutdown.trigger().unwrap();

        // Read back from the pipe read end to prove data was written
        let mut buf = [0u8; 8];
        let n = unsafe {
            libc::read(fds[0], buf.as_mut_ptr().cast(), 8)
        };
        assert_eq!(n, 8, "trigger should write exactly 8 bytes (u64)");
        let val = u64::from_ne_bytes(buf);
        assert_eq!(val, 1, "trigger should write the value 1");

        unsafe { libc::close(fds[0]) };
    }

    // Verify trigger can be called multiple times.
    #[test]
    fn eventfd_shutdown_trigger_multiple() {
        let mut fds = [0i32; 2];
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(ret, 0);
        let shutdown = unsafe { EventfdShutdown::from_raw_fd(fds[1]) };
        shutdown.trigger().unwrap();
        shutdown.trigger().unwrap();

        let mut buf = [0u8; 8];
        for expected in [1u64, 1u64] {
            let n = unsafe { libc::read(fds[0], buf.as_mut_ptr().cast(), 8) };
            assert_eq!(n, 8);
            assert_eq!(u64::from_ne_bytes(buf), expected);
        }
        unsafe { libc::close(fds[0]) };
    }

    // Verify Shutdown trait is object-safe (can be used as dyn Shutdown).
    #[test]
    fn shutdown_trait_is_object_safe() {
        fn assert_object_safe(_: &dyn Shutdown) {}
        let mock = MockShutdown::new();
        assert_object_safe(&mock);
    }

    // MockShutdown default construction
    #[test]
    fn mock_shutdown_default() {
        let s = MockShutdown::default();
        assert_eq!(s.triggered(), 0);
        assert!(!s.fail.load(std::sync::atomic::Ordering::SeqCst));
    }

    // Verify trigger count increments even when fail is set
    #[test]
    fn mock_shutdown_fail_still_increments() {
        let s = MockShutdown::new();
        s.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = s.trigger();
        let _ = s.trigger();
        assert_eq!(s.triggered(), 2);
    }
}
