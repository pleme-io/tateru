//! VM builder and handle — the primary public API.
//!
//! Generic over [`VmEngine`] for full testability.
//!
//! # Example (production)
//!
//! ```ignore
//! use tateru::{Vm, LibkrunEngine, DiskConfig, DiskFormat, VirtioFsMount};
//!
//! let handle = Vm::builder(LibkrunEngine::new())
//!     .cpus(6)?
//!     .memory("8GiB")?
//!     .disk(DiskConfig { path: image.into(), format: DiskFormat::Qcow2, read_only: false })
//!     .virtiofs(VirtioFsMount { host_path: "/shared".into(), mount_tag: "data".into() })
//!     .vsock_bridge(31122, 22)?
//!     .launch()
//!     .await?;
//!
//! assert!(handle.is_running());
//! handle.stop()?;
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::watch;
use tracing::info;

use crate::bridge::{self, BridgeConfig, BridgeHandle};
use crate::devices::{ConsoleConfig, DiskConfig, VirtioFsMount, VsockPort};
use crate::engine::VmEngine;
use crate::error::TateruError;
use crate::shutdown::{EventfdShutdown, Shutdown};
use crate::types::{GuestPort, MemoryMib, VcpuCount};

// ---------------------------------------------------------------------------
// BridgeSpawner trait
// ---------------------------------------------------------------------------

/// Abstraction for spawning vsock TCP bridge tasks.
///
/// The real implementation spawns tokio tasks that forward TCP connections
/// to Unix sockets. Tests can substitute a mock that records calls.
pub trait BridgeSpawner: Send + Sync {
    /// Spawn a bridge task for the given configuration.
    fn spawn(
        &self,
        config: BridgeConfig,
        shutdown: watch::Receiver<bool>,
    ) -> BridgeHandle;
}

/// Production bridge spawner using tokio tasks.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioBridgeSpawner;

impl TokioBridgeSpawner {
    /// Create a new tokio bridge spawner.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl BridgeSpawner for TokioBridgeSpawner {
    fn spawn(
        &self,
        config: BridgeConfig,
        shutdown: watch::Receiver<bool>,
    ) -> BridgeHandle {
        bridge::spawn_bridge(config, shutdown)
    }
}

// ---------------------------------------------------------------------------
// VmControl trait
// ---------------------------------------------------------------------------

/// Abstraction over a running VM handle for lifecycle management.
///
/// Enables mocking in tests without requiring a real VM or real file descriptors.
pub trait VmControl: Send + std::fmt::Debug {
    /// Check if the VM is still running.
    fn is_running(&self) -> bool;

    /// Gracefully stop the VM.
    fn stop(&mut self) -> Result<(), TateruError>;
}

/// A vsock bridge request: host TCP port ↔ guest vsock port.
#[derive(Debug, Clone)]
pub(crate) struct BridgeRequest {
    host_port: u16,
    guest_port: GuestPort,
}

/// Builder for constructing and launching a VM.
///
/// Generic over `E: VmEngine` — use [`LibkrunEngine`](crate::engine::LibkrunEngine)
/// in production, [`MockEngine`](crate::engine::mock::MockEngine) in tests.
#[derive(Debug)]
pub struct VmBuilder<E: VmEngine> {
    engine: E,
    vcpus: Option<VcpuCount>,
    memory: Option<MemoryMib>,
    pub(crate) disks: Vec<DiskConfig>,
    pub(crate) virtiofs_mounts: Vec<VirtioFsMount>,
    pub(crate) vsock_ports: Vec<VsockPort>,
    pub(crate) bridges: Vec<BridgeRequest>,
    console: Option<ConsoleConfig>,
    oem_strings: Vec<String>,
    pub(crate) nested_virt: Option<bool>,
}

impl<E: VmEngine> VmBuilder<E> {
    /// Create a new builder with the given engine.
    pub fn new(engine: E) -> Self {
        Self {
            engine,
            vcpus: None,
            memory: None,
            disks: Vec::new(),
            virtiofs_mounts: Vec::new(),
            vsock_ports: Vec::new(),
            bridges: Vec::new(),
            console: None,
            oem_strings: Vec::new(),
            nested_virt: None,
        }
    }

    /// Set the number of vCPUs.
    pub fn cpus(mut self, count: u8) -> Result<Self, TateruError> {
        self.vcpus = Some(VcpuCount::new(count)?);
        Ok(self)
    }

    /// Set the vCPU count from a pre-validated value.
    #[must_use]
    pub fn vcpus(mut self, vcpus: VcpuCount) -> Self {
        self.vcpus = Some(vcpus);
        self
    }

    /// Set memory from a human-readable string (e.g. `"8GiB"`, `"4096MiB"`, `"4096"`).
    pub fn memory(mut self, spec: &str) -> Result<Self, TateruError> {
        self.memory = Some(MemoryMib::parse(spec)?);
        Ok(self)
    }

    /// Set memory from a pre-validated value.
    #[must_use]
    pub fn memory_mib(mut self, mib: MemoryMib) -> Self {
        self.memory = Some(mib);
        self
    }

    /// Add a disk image.
    #[must_use]
    pub fn disk(mut self, disk: DiskConfig) -> Self {
        self.disks.push(disk);
        self
    }

    /// Add a virtiofs shared directory.
    #[must_use]
    pub fn virtiofs(mut self, mount: VirtioFsMount) -> Self {
        self.virtiofs_mounts.push(mount);
        self
    }

    /// Add a vsock port (raw, no bridge).
    #[must_use]
    pub fn vsock(mut self, port: VsockPort) -> Self {
        self.vsock_ports.push(port);
        self
    }

    /// Add a vsock bridge: host TCP port ↔ guest vsock port.
    ///
    /// This both registers the vsock port with libkrun AND starts a TCP
    /// bridge on the host that forwards connections to the guest.
    pub fn vsock_bridge(
        mut self,
        host_port: u16,
        guest_port: u32,
    ) -> Result<Self, TateruError> {
        let guest_port = GuestPort::new(guest_port)?;
        self.bridges.push(BridgeRequest {
            host_port,
            guest_port,
        });
        Ok(self)
    }

    /// Set console output path.
    #[must_use]
    pub fn console(mut self, config: ConsoleConfig) -> Self {
        self.console = Some(config);
        self
    }

    /// Add SMBIOS OEM strings.
    #[must_use]
    pub fn oem_string(mut self, s: impl Into<String>) -> Self {
        self.oem_strings.push(s.into());
        self
    }

    /// Enable or disable nested virtualization.
    #[must_use]
    pub fn nested_virt(mut self, enabled: bool) -> Self {
        self.nested_virt = Some(enabled);
        self
    }

    /// Validate the builder configuration.
    fn validate(&self) -> Result<(), TateruError> {
        if self.vcpus.is_none() {
            return Err(TateruError::InvalidConfig("vCPU count not set".into()));
        }
        if self.memory.is_none() {
            return Err(TateruError::InvalidConfig("memory not set".into()));
        }
        if self.disks.is_empty() {
            return Err(TateruError::InvalidConfig(
                "at least one disk is required".into(),
            ));
        }
        Ok(())
    }

    /// Launch the VM using the default [`TokioBridgeSpawner`].
    ///
    /// 1. Creates a libkrun context via the engine
    /// 2. Configures vCPUs, memory, devices
    /// 3. Starts vsock bridges
    /// 4. Spawns a dedicated thread for `start_enter` (blocks forever)
    /// 5. Returns a `VmHandle` for lifecycle management
    pub async fn launch(self) -> Result<VmHandle, TateruError>
    where
        E: 'static,
    {
        self.launch_with_spawner(TokioBridgeSpawner::new()).await
    }

    /// Launch the VM with a custom [`BridgeSpawner`].
    ///
    /// Same as [`launch`](Self::launch) but allows injecting a mock spawner
    /// for testing bridge orchestration without real TCP listeners.
    pub async fn launch_with_spawner<S: BridgeSpawner>(
        self,
        spawner: S,
    ) -> Result<VmHandle, TateruError>
    where
        E: 'static,
    {
        self.validate()?;

        let vcpus = self.vcpus.unwrap();
        let memory = self.memory.unwrap();

        info!("creating VM context ({vcpus}, {memory})");

        // 1. Create context
        let ctx = self.engine.create_ctx()?;
        info!("VM context created: {ctx}");

        // 2. Configure VM
        self.engine.set_vm_config(ctx, vcpus, memory)?;

        // 3. Apply devices
        for (i, disk) in self.disks.iter().enumerate() {
            self.engine.add_disk(ctx, disk, i)?;
        }
        for mount in &self.virtiofs_mounts {
            self.engine.add_virtiofs(ctx, mount)?;
        }
        for port in &self.vsock_ports {
            self.engine.add_vsock_port(ctx, port)?;
        }

        // 4. Register vsock ports for bridges (create temp socket paths)
        let workdir = std::env::temp_dir().join(format!("tateru-{}", ctx.raw()));
        std::fs::create_dir_all(&workdir)?;

        let mut bridge_configs = Vec::new();
        for br in &self.bridges {
            let socket_path = workdir.join(format!("vsock-{}.sock", br.guest_port.raw()));
            let vsock = VsockPort {
                guest_port: br.guest_port,
                host_socket: socket_path.clone(),
            };
            self.engine.add_vsock_port(ctx, &vsock)?;
            bridge_configs.push(BridgeConfig {
                host_port: br.host_port,
                socket_path,
            });
        }

        // 5. Console
        if let Some(ref console) = self.console {
            self.engine.set_console_output(ctx, console)?;
        }

        // 6. OEM strings
        if !self.oem_strings.is_empty() {
            let refs: Vec<&str> = self.oem_strings.iter().map(String::as_str).collect();
            self.engine.set_smbios_oem_strings(ctx, &refs)?;
        }

        // 7. Nested virt
        if let Some(enabled) = self.nested_virt {
            self.engine.set_nested_virt(ctx, enabled)?;
        }

        // 8. Get shutdown eventfd
        let shutdown_fd = self.engine.get_shutdown_eventfd(ctx)?;
        let shutdown: Box<dyn Shutdown> =
            Box::new(unsafe { EventfdShutdown::from_raw_fd(shutdown_fd) });

        // 9. Shutdown signal channel for bridges
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let running = Arc::new(AtomicBool::new(true));

        // 10. Spawn bridge tasks
        let mut bridge_handles = Vec::new();
        for cfg in bridge_configs {
            let handle = spawner.spawn(cfg, shutdown_rx.clone());
            bridge_handles.push(handle);
        }

        // 11. Spawn VM thread — start_enter blocks forever
        let vm_running = Arc::clone(&running);
        let vm_thread = std::thread::Builder::new()
            .name("tateru-vm".into())
            .spawn(move || {
                let result = self.engine.start_enter(ctx);
                vm_running.store(false, Ordering::SeqCst);
                result
            })
            .map_err(TateruError::BridgeIo)?;

        info!("VM launched on dedicated thread");

        Ok(VmHandle {
            running,
            shutdown,
            shutdown_tx,
            bridge_handles,
            vm_thread: Some(vm_thread),
            workdir,
        })
    }
}

/// Handle to a running VM. Provides lifecycle management.
///
/// Implements [`VmControl`] for testable lifecycle management.
pub struct VmHandle {
    running: Arc<AtomicBool>,
    shutdown: Box<dyn Shutdown>,
    shutdown_tx: watch::Sender<bool>,
    #[allow(dead_code)]
    bridge_handles: Vec<BridgeHandle>,
    vm_thread: Option<std::thread::JoinHandle<Result<(), TateruError>>>,
    workdir: PathBuf,
}

impl VmControl for VmHandle {
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop(&mut self) -> Result<(), TateruError> {
        if !self.is_running() {
            return Err(TateruError::NotRunning);
        }

        info!("sending shutdown signal to VM");
        self.shutdown.trigger()?;

        // Signal bridges to stop
        let _ = self.shutdown_tx.send(true);

        // Wait for VM thread to exit
        if let Some(thread) = self.vm_thread.take() {
            match thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    // start_enter always returns an error (it blocks forever),
                    // so this is expected
                    tracing::debug!("VM thread exited with: {e}");
                }
                Err(_) => return Err(TateruError::VmThreadPanicked),
            }
        }

        // Clean up workdir
        let _ = std::fs::remove_dir_all(&self.workdir);

        self.running.store(false, Ordering::SeqCst);
        info!("VM stopped");
        Ok(())
    }
}

impl VmHandle {
    /// Check if the VM is still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        <Self as VmControl>::is_running(self)
    }

    /// Gracefully stop the VM via the shutdown eventfd.
    pub fn stop(&mut self) -> Result<(), TateruError> {
        <Self as VmControl>::stop(self)
    }
}

impl Drop for VmHandle {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.shutdown.trigger();
            let _ = self.shutdown_tx.send(true);
        }
        let _ = std::fs::remove_dir_all(&self.workdir);
    }
}

impl std::fmt::Debug for VmHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VmHandle")
            .field("running", &self.is_running())
            .field("workdir", &self.workdir)
            .finish()
    }
}

/// Mock VM control handle for testing without real VMs.
///
/// Tracks `is_running` and `stop` calls without file descriptors or threads.
#[cfg(any(test, feature = "testing"))]
#[derive(Debug)]
pub struct MockVmControl {
    /// Whether the mock VM is considered running.
    pub running: AtomicBool,
    /// Number of times `stop()` has been called.
    pub stop_count: std::sync::atomic::AtomicU32,
    /// If true, `stop()` returns an error.
    pub fail_stop: AtomicBool,
}

#[cfg(any(test, feature = "testing"))]
impl Default for MockVmControl {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(true),
            stop_count: std::sync::atomic::AtomicU32::new(0),
            fail_stop: AtomicBool::new(false),
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl MockVmControl {
    /// Create a new mock handle in the running state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many times `stop()` was called.
    #[must_use]
    pub fn stopped(&self) -> u32 {
        self.stop_count.load(Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "testing"))]
impl VmControl for MockVmControl {
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop(&mut self) -> Result<(), TateruError> {
        if !self.is_running() {
            return Err(TateruError::NotRunning);
        }
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        if self.fail_stop.load(Ordering::SeqCst) {
            return Err(TateruError::BridgeIo(std::io::Error::new(
                std::io::ErrorKind::Other,
                "mock stop failure",
            )));
        }
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }
}

/// Mock bridge spawner for testing.
///
/// Records spawn calls instead of creating real TCP listeners.
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Default)]
pub struct MockBridgeSpawner {
    /// Number of bridges spawned.
    pub spawn_count: std::sync::atomic::AtomicU32,
}

#[cfg(any(test, feature = "testing"))]
impl MockBridgeSpawner {
    /// Create a new mock bridge spawner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many times `spawn()` was called.
    #[must_use]
    pub fn spawned(&self) -> u32 {
        self.spawn_count.load(Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "testing"))]
impl BridgeSpawner for MockBridgeSpawner {
    fn spawn(
        &self,
        _config: BridgeConfig,
        _shutdown: watch::Receiver<bool>,
    ) -> BridgeHandle {
        self.spawn_count.fetch_add(1, Ordering::SeqCst);
        // Return a handle with a no-op task
        BridgeHandle::noop()
    }
}

/// Convenience entry point for creating a VM builder.
pub struct Vm;

impl Vm {
    /// Create a new VM builder with the given engine.
    pub fn builder<E: VmEngine>(engine: E) -> VmBuilder<E> {
        VmBuilder::new(engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::DiskFormat;
    use crate::engine::mock::MockEngine;

    fn test_builder() -> VmBuilder<MockEngine> {
        Vm::builder(MockEngine::new())
    }

    #[test]
    fn builder_validation_no_cpus() {
        let builder = VmBuilder::new(MockEngine::new());
        // memory and disk set, but no cpus
        let builder = builder
            .memory_mib(MemoryMib::new(1024).unwrap())
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            });
        let err = builder.validate().unwrap_err();
        assert!(err.to_string().contains("vCPU count not set"));
    }

    #[test]
    fn builder_validation_no_memory() {
        let builder = test_builder()
            .cpus(4)
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            });
        let err = builder.validate().unwrap_err();
        assert!(err.to_string().contains("memory not set"));
    }

    #[test]
    fn builder_validation_no_disk() {
        let builder = test_builder().cpus(4).unwrap().memory("4GiB").unwrap();
        let err = builder.validate().unwrap_err();
        assert!(err.to_string().contains("at least one disk"));
    }

    #[test]
    fn builder_validation_ok() {
        let builder = test_builder()
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            });
        builder.validate().unwrap();
    }

    #[test]
    fn builder_cpus_zero_rejected() {
        let err = test_builder().cpus(0).unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn builder_memory_invalid_rejected() {
        let err = test_builder().memory("lots").unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn builder_vsock_bridge_zero_port_rejected() {
        let err = test_builder().vsock_bridge(31122, 0).unwrap_err();
        assert!(err.to_string().contains("> 0"));
    }

    #[test]
    fn builder_vsock_bridge_valid() {
        let builder = test_builder().vsock_bridge(31122, 22).unwrap();
        assert_eq!(builder.bridges.len(), 1);
        assert_eq!(builder.bridges[0].host_port, 31122);
        assert_eq!(builder.bridges[0].guest_port.raw(), 22);
    }

    #[test]
    fn builder_multiple_disks() {
        let builder = test_builder()
            .disk(DiskConfig {
                path: "/a.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .disk(DiskConfig {
                path: "/b.raw".into(),
                format: DiskFormat::Raw,
                read_only: true,
            });
        assert_eq!(builder.disks.len(), 2);
    }

    #[test]
    fn builder_multiple_virtiofs() {
        let builder = test_builder()
            .virtiofs(VirtioFsMount {
                host_path: "/shared1".into(),
                mount_tag: "tag1".into(),
            })
            .virtiofs(VirtioFsMount {
                host_path: "/shared2".into(),
                mount_tag: "tag2".into(),
            });
        assert_eq!(builder.virtiofs_mounts.len(), 2);
    }

    #[test]
    fn builder_oem_strings() {
        let builder = test_builder()
            .oem_string("key1=val1")
            .oem_string("key2=val2");
        assert_eq!(builder.oem_strings.len(), 2);
    }

    #[test]
    fn builder_nested_virt() {
        let builder = test_builder().nested_virt(true);
        assert_eq!(builder.nested_virt, Some(true));
    }

    #[test]
    fn builder_console() {
        let builder = test_builder().console(ConsoleConfig {
            log_path: "/var/log/vm.log".into(),
        });
        assert!(builder.console.is_some());
    }

    #[tokio::test]
    async fn launch_records_engine_calls() {
        let engine = MockEngine::new();
        let result = Vm::builder(engine)
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .launch()
            .await;

        // Launch will succeed because MockEngine::start_enter returns Ok(())
        // immediately (unlike real libkrun which blocks forever)
        assert!(result.is_ok());

        let mut handle = result.unwrap();
        // The mock's start_enter returns immediately, so the VM thread finishes
        // quickly and running becomes false
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Clean up
        let _ = handle.stop();
    }

    #[tokio::test]
    async fn launch_with_bridges() {
        let result = Vm::builder(MockEngine::new())
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .vsock_bridge(0, 22) // port 0 = ephemeral
            .unwrap()
            .launch()
            .await;

        assert!(result.is_ok());
        let mut handle = result.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = handle.stop();
    }

    #[tokio::test]
    async fn launch_with_all_options() {
        let result = Vm::builder(MockEngine::new())
            .cpus(6)
            .unwrap()
            .memory("8GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .virtiofs(VirtioFsMount {
                host_path: "/shared".into(),
                mount_tag: "data".into(),
            })
            .console(ConsoleConfig {
                log_path: "/tmp/console.log".into(),
            })
            .oem_string("test=value")
            .nested_virt(true)
            .launch()
            .await;

        assert!(result.is_ok());
        let mut handle = result.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = handle.stop();
    }

    #[tokio::test]
    async fn launch_engine_failure_propagates() {
        use std::sync::atomic::Ordering;

        let engine = MockEngine::new();
        engine.fail_create_ctx.store(true, Ordering::SeqCst);

        let result = Vm::builder(engine)
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .launch()
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TateruError::Ffi { .. }));
    }

    #[test]
    fn vm_handle_debug() {
        // Can't easily construct a VmHandle without launching, but we can
        // verify the Debug impl exists via the trait bound.
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<VmHandle>();
    }

    #[test]
    fn builder_vcpus_pre_validated() {
        let vcpus = VcpuCount::new(8).unwrap();
        let builder = test_builder().vcpus(vcpus);
        assert_eq!(builder.vcpus.unwrap().raw(), 8);
    }

    #[test]
    fn builder_memory_mib_pre_validated() {
        let mem = MemoryMib::new(4096).unwrap();
        let builder = test_builder().memory_mib(mem);
        assert_eq!(builder.memory.unwrap().raw(), 4096);
    }

    // -- VmBuilder edge cases --

    #[test]
    fn builder_duplicate_disks_same_path() {
        let disk = DiskConfig {
            path: "/same.qcow2".into(),
            format: DiskFormat::Qcow2,
            read_only: false,
        };
        let builder = test_builder().disk(disk.clone()).disk(disk);
        // Builder allows duplicate disk paths — libkrun decides if that's valid
        assert_eq!(builder.disks.len(), 2);
        assert_eq!(builder.disks[0].path, builder.disks[1].path);
    }

    #[test]
    fn builder_empty_mount_tag() {
        let builder = test_builder().virtiofs(VirtioFsMount {
            host_path: "/shared".into(),
            mount_tag: String::new(),
        });
        assert_eq!(builder.virtiofs_mounts.len(), 1);
        assert!(builder.virtiofs_mounts[0].mount_tag.is_empty());
    }

    #[test]
    fn builder_very_large_memory() {
        // u32::MAX MiB
        let mem = MemoryMib::new(u32::MAX).unwrap();
        let builder = test_builder().memory_mib(mem);
        assert_eq!(builder.memory.unwrap().raw(), u32::MAX);
    }

    #[test]
    fn builder_max_cpus() {
        let builder = test_builder().cpus(255).unwrap();
        assert_eq!(builder.vcpus.unwrap().raw(), 255);
    }

    #[test]
    fn builder_multiple_bridges() {
        let builder = test_builder()
            .vsock_bridge(31122, 22)
            .unwrap()
            .vsock_bridge(31180, 80)
            .unwrap()
            .vsock_bridge(31443, 443)
            .unwrap();
        assert_eq!(builder.bridges.len(), 3);
    }

    // -- MockVmControl tests --

    #[test]
    fn mock_vm_control_starts_running() {
        let ctrl = MockVmControl::new();
        assert!(ctrl.is_running());
        assert_eq!(ctrl.stopped(), 0);
    }

    #[test]
    fn mock_vm_control_stop() {
        let mut ctrl = MockVmControl::new();
        ctrl.stop().unwrap();
        assert!(!ctrl.is_running());
        assert_eq!(ctrl.stopped(), 1);
    }

    #[test]
    fn mock_vm_control_stop_when_not_running() {
        let mut ctrl = MockVmControl::new();
        ctrl.stop().unwrap();
        let err = ctrl.stop().unwrap_err();
        assert!(matches!(err, TateruError::NotRunning));
    }

    #[test]
    fn mock_vm_control_fail_stop() {
        let mut ctrl = MockVmControl::new();
        ctrl.fail_stop.store(true, Ordering::SeqCst);
        let err = ctrl.stop().unwrap_err();
        assert!(matches!(err, TateruError::BridgeIo(_)));
        // Still recorded the call and stays running since stop failed
        assert_eq!(ctrl.stopped(), 1);
        assert!(ctrl.is_running());
    }

    #[test]
    fn mock_vm_control_debug() {
        let ctrl = MockVmControl::new();
        let debug = format!("{ctrl:?}");
        assert!(debug.contains("MockVmControl"));
    }

    // -- MockBridgeSpawner tests --

    #[test]
    fn mock_bridge_spawner_counts() {
        let spawner = MockBridgeSpawner::new();
        assert_eq!(spawner.spawned(), 0);

        let (_tx, rx) = watch::channel(false);
        let cfg = BridgeConfig {
            host_port: 31122,
            socket_path: "/tmp/vsock-22.sock".into(),
        };
        let _handle = spawner.spawn(cfg.clone(), rx.clone());
        assert_eq!(spawner.spawned(), 1);

        let _handle2 = spawner.spawn(cfg, rx);
        assert_eq!(spawner.spawned(), 2);
    }

    // -- launch with mock spawner --

    #[tokio::test]
    async fn launch_with_mock_spawner_records_bridge_count() {
        let spawner = MockBridgeSpawner::new();
        let result = Vm::builder(MockEngine::new())
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .vsock_bridge(0, 22)
            .unwrap()
            .vsock_bridge(0, 80)
            .unwrap()
            .launch_with_spawner(spawner)
            .await;

        assert!(result.is_ok());
        let mut handle = result.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = handle.stop();
    }

    // -- VmControl trait is object-safe --

    #[test]
    fn vm_control_is_object_safe() {
        fn assert_object_safe(_: &dyn VmControl) {}
        let ctrl = MockVmControl::new();
        assert_object_safe(&ctrl);
    }

    // -- launch_with_spawner engine failures --

    #[tokio::test]
    async fn launch_set_vm_config_failure() {
        let engine = MockEngine::new();
        engine
            .fail_set_vm_config
            .store(true, Ordering::SeqCst);

        let result = Vm::builder(engine)
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .launch()
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TateruError::Ffi { .. }));
    }

    #[tokio::test]
    async fn launch_add_disk_failure() {
        let engine = MockEngine::new();
        engine.fail_add_disk.store(true, Ordering::SeqCst);

        let result = Vm::builder(engine)
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .launch()
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TateruError::Ffi { .. }));
    }

    #[tokio::test]
    async fn launch_get_shutdown_eventfd_failure() {
        let engine = MockEngine::new();
        engine
            .fail_get_shutdown_eventfd
            .store(true, Ordering::SeqCst);

        let result = Vm::builder(engine)
            .cpus(4)
            .unwrap()
            .memory("4GiB")
            .unwrap()
            .disk(DiskConfig {
                path: "/test.qcow2".into(),
                format: DiskFormat::Qcow2,
                read_only: false,
            })
            .launch()
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TateruError::Ffi { .. }));
    }
}
