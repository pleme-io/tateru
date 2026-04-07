//! VM engine trait and libkrun FFI implementation.
//!
//! The [`VmEngine`] trait abstracts all libkrun FFI calls behind a safe,
//! type-strict boundary. The real implementation ([`LibkrunEngine`]) calls
//! through to the C library; tests can substitute [`MockEngine`].

use crate::devices::{ConsoleConfig, DiskConfig, VirtioFsMount, VsockPort};
use crate::error::TateruError;
use crate::types::{CtxId, LogLevel, MemoryMib, VcpuCount};

/// Abstraction over VM engine operations (FFI boundary).
///
/// Every libkrun C API call is represented as a method on this trait,
/// taking and returning strong newtypes instead of raw integers.
/// This trait is the **sole** boundary through which FFI is accessed.
pub trait VmEngine: Send + Sync {
    /// Create a new VM context.
    fn create_ctx(&self) -> Result<CtxId, TateruError>;

    /// Configure vCPUs and memory for a context.
    fn set_vm_config(
        &self,
        ctx: CtxId,
        vcpus: VcpuCount,
        memory: MemoryMib,
    ) -> Result<(), TateruError>;

    /// Attach a disk image to the VM.
    fn add_disk(
        &self,
        ctx: CtxId,
        disk: &DiskConfig,
        index: usize,
    ) -> Result<(), TateruError>;

    /// Add a virtiofs shared directory.
    fn add_virtiofs(
        &self,
        ctx: CtxId,
        mount: &VirtioFsMount,
    ) -> Result<(), TateruError>;

    /// Register a vsock port backed by a Unix socket.
    fn add_vsock_port(
        &self,
        ctx: CtxId,
        port: &VsockPort,
    ) -> Result<(), TateruError>;

    /// Redirect console output to a file.
    fn set_console_output(
        &self,
        ctx: CtxId,
        console: &ConsoleConfig,
    ) -> Result<(), TateruError>;

    /// Get the shutdown eventfd. Must be called before `start_enter`.
    ///
    /// Returns the raw file descriptor.
    fn get_shutdown_eventfd(&self, ctx: CtxId) -> Result<i32, TateruError>;

    /// Start the VM. **Blocks the calling thread forever.**
    ///
    /// Only returns on error.
    fn start_enter(&self, ctx: CtxId) -> Result<(), TateruError>;

    /// Set the libkrun log level.
    fn set_log_level(&self, level: LogLevel) -> Result<(), TateruError>;

    /// Check if nested virtualization is supported (macOS only).
    fn check_nested_virt(&self) -> Result<bool, TateruError>;

    /// Enable or disable nested virtualization for a context.
    fn set_nested_virt(&self, ctx: CtxId, enabled: bool) -> Result<(), TateruError>;

    /// Set SMBIOS OEM strings for the VM.
    fn set_smbios_oem_strings(
        &self,
        ctx: CtxId,
        strings: &[&str],
    ) -> Result<(), TateruError>;
}

// ---------------------------------------------------------------------------
// Real libkrun FFI implementation
// ---------------------------------------------------------------------------

/// Production [`VmEngine`] that calls through to the libkrun-efi C library.
///
/// This is a zero-sized type — all state lives in libkrun's C library.
#[derive(Debug, Default)]
pub struct LibkrunEngine;

impl LibkrunEngine {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl VmEngine for LibkrunEngine {
    fn create_ctx(&self) -> Result<CtxId, TateruError> {
        crate::ffi::create_ctx().map(CtxId)
    }

    fn set_vm_config(
        &self,
        ctx: CtxId,
        vcpus: VcpuCount,
        memory: MemoryMib,
    ) -> Result<(), TateruError> {
        crate::ffi::set_vm_config(ctx.0, vcpus.raw(), memory.raw())
    }

    fn add_disk(
        &self,
        ctx: CtxId,
        disk: &DiskConfig,
        index: usize,
    ) -> Result<(), TateruError> {
        let block_id = format!("disk{index}");
        crate::ffi::add_disk(
            ctx.0,
            &block_id,
            &disk.path,
            disk.format.to_ffi(),
            disk.read_only,
        )
    }

    fn add_virtiofs(
        &self,
        ctx: CtxId,
        mount: &VirtioFsMount,
    ) -> Result<(), TateruError> {
        crate::ffi::add_virtiofs(ctx.0, &mount.mount_tag, &mount.host_path)
    }

    fn add_vsock_port(
        &self,
        ctx: CtxId,
        port: &VsockPort,
    ) -> Result<(), TateruError> {
        crate::ffi::add_vsock_port(ctx.0, port.guest_port.raw(), &port.host_socket)
    }

    fn set_console_output(
        &self,
        ctx: CtxId,
        console: &ConsoleConfig,
    ) -> Result<(), TateruError> {
        crate::ffi::set_console_output(ctx.0, &console.log_path)
    }

    fn get_shutdown_eventfd(&self, ctx: CtxId) -> Result<i32, TateruError> {
        crate::ffi::get_shutdown_eventfd(ctx.0)
    }

    fn start_enter(&self, ctx: CtxId) -> Result<(), TateruError> {
        crate::ffi::start_enter(ctx.0)
    }

    fn set_log_level(&self, level: LogLevel) -> Result<(), TateruError> {
        crate::ffi::set_log_level(level.raw())
    }

    fn check_nested_virt(&self) -> Result<bool, TateruError> {
        crate::ffi::check_nested_virt()
    }

    fn set_nested_virt(&self, ctx: CtxId, enabled: bool) -> Result<(), TateruError> {
        crate::ffi::set_nested_virt(ctx.0, enabled)
    }

    fn set_smbios_oem_strings(
        &self,
        ctx: CtxId,
        strings: &[&str],
    ) -> Result<(), TateruError> {
        crate::ffi::set_smbios_oem_strings(ctx.0, strings)
    }
}

// ---------------------------------------------------------------------------
// Mock engine for testing
// ---------------------------------------------------------------------------

/// Mock [`VmEngine`] for unit testing without libkrun.
///
/// Records all calls and returns configurable results.
#[cfg(any(test, feature = "testing"))]
pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex;

    /// Recorded engine call for assertion in tests.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EngineCall {
        CreateCtx,
        SetVmConfig {
            ctx: u32,
            vcpus: u8,
            memory_mib: u32,
        },
        AddDisk {
            ctx: u32,
            index: usize,
        },
        AddVirtiofs {
            ctx: u32,
            tag: String,
        },
        AddVsockPort {
            ctx: u32,
            guest_port: u32,
        },
        SetConsoleOutput {
            ctx: u32,
        },
        GetShutdownEventfd {
            ctx: u32,
        },
        StartEnter {
            ctx: u32,
        },
        SetLogLevel {
            level: u32,
        },
        CheckNestedVirt,
        SetNestedVirt {
            ctx: u32,
            enabled: bool,
        },
        SetSmbiosOemStrings {
            ctx: u32,
            count: usize,
        },
    }

    /// Mock VM engine that records calls and can be configured to fail.
    #[derive(Debug)]
    pub struct MockEngine {
        pub calls: Mutex<Vec<EngineCall>>,
        pub next_ctx_id: AtomicU32,
        pub fail_create_ctx: AtomicBool,
        pub fail_start_enter: AtomicBool,
        pub fail_set_vm_config: AtomicBool,
        pub fail_add_disk: AtomicBool,
        pub fail_get_shutdown_eventfd: AtomicBool,
        pub nested_virt_supported: AtomicBool,
    }

    impl MockEngine {
        #[must_use]
        pub fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                next_ctx_id: AtomicU32::new(0),
                fail_create_ctx: AtomicBool::new(false),
                fail_start_enter: AtomicBool::new(false),
                fail_set_vm_config: AtomicBool::new(false),
                fail_add_disk: AtomicBool::new(false),
                fail_get_shutdown_eventfd: AtomicBool::new(false),
                nested_virt_supported: AtomicBool::new(true),
            }
        }

        /// Get all recorded calls.
        pub fn recorded_calls(&self) -> Vec<EngineCall> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, call: EngineCall) {
            self.calls.lock().unwrap().push(call);
        }
    }

    impl Default for MockEngine {
        fn default() -> Self {
            Self::new()
        }
    }

    impl VmEngine for MockEngine {
        fn create_ctx(&self) -> Result<CtxId, TateruError> {
            self.record(EngineCall::CreateCtx);
            if self.fail_create_ctx.load(Ordering::SeqCst) {
                return Err(TateruError::Ffi {
                    function: "krun_create_ctx",
                    code: -1,
                });
            }
            let id = self.next_ctx_id.fetch_add(1, Ordering::SeqCst);
            Ok(CtxId(id))
        }

        fn set_vm_config(
            &self,
            ctx: CtxId,
            vcpus: VcpuCount,
            memory: MemoryMib,
        ) -> Result<(), TateruError> {
            self.record(EngineCall::SetVmConfig {
                ctx: ctx.0,
                vcpus: vcpus.raw(),
                memory_mib: memory.raw(),
            });
            if self.fail_set_vm_config.load(Ordering::SeqCst) {
                return Err(TateruError::Ffi {
                    function: "krun_set_vm_config",
                    code: -22,
                });
            }
            Ok(())
        }

        fn add_disk(
            &self,
            ctx: CtxId,
            _disk: &DiskConfig,
            index: usize,
        ) -> Result<(), TateruError> {
            self.record(EngineCall::AddDisk {
                ctx: ctx.0,
                index,
            });
            if self.fail_add_disk.load(Ordering::SeqCst) {
                return Err(TateruError::Ffi {
                    function: "krun_add_disk2",
                    code: -22,
                });
            }
            Ok(())
        }

        fn add_virtiofs(
            &self,
            ctx: CtxId,
            mount: &VirtioFsMount,
        ) -> Result<(), TateruError> {
            self.record(EngineCall::AddVirtiofs {
                ctx: ctx.0,
                tag: mount.mount_tag.clone(),
            });
            Ok(())
        }

        fn add_vsock_port(
            &self,
            ctx: CtxId,
            port: &VsockPort,
        ) -> Result<(), TateruError> {
            self.record(EngineCall::AddVsockPort {
                ctx: ctx.0,
                guest_port: port.guest_port.raw(),
            });
            Ok(())
        }

        fn set_console_output(
            &self,
            ctx: CtxId,
            _console: &ConsoleConfig,
        ) -> Result<(), TateruError> {
            self.record(EngineCall::SetConsoleOutput { ctx: ctx.0 });
            Ok(())
        }

        fn get_shutdown_eventfd(&self, ctx: CtxId) -> Result<i32, TateruError> {
            self.record(EngineCall::GetShutdownEventfd { ctx: ctx.0 });
            if self.fail_get_shutdown_eventfd.load(Ordering::SeqCst) {
                return Err(TateruError::Ffi {
                    function: "krun_get_shutdown_eventfd",
                    code: -1,
                });
            }
            // Create a real pipe so OwnedFd doesn't abort on close.
            // We only need the write end to act as a dummy eventfd.
            let mut fds = [0i32; 2];
            let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
            if ret != 0 {
                return Err(TateruError::BridgeIo(std::io::Error::last_os_error()));
            }
            // Close read end — we only need the write end
            unsafe { libc::close(fds[0]) };
            Ok(fds[1])
        }

        fn start_enter(&self, ctx: CtxId) -> Result<(), TateruError> {
            self.record(EngineCall::StartEnter { ctx: ctx.0 });
            if self.fail_start_enter.load(Ordering::SeqCst) {
                return Err(TateruError::Ffi {
                    function: "krun_start_enter",
                    code: -22,
                });
            }
            // In mock, we just return Ok — real impl blocks forever
            Ok(())
        }

        fn set_log_level(&self, level: LogLevel) -> Result<(), TateruError> {
            self.record(EngineCall::SetLogLevel {
                level: level.raw(),
            });
            Ok(())
        }

        fn check_nested_virt(&self) -> Result<bool, TateruError> {
            self.record(EngineCall::CheckNestedVirt);
            Ok(self.nested_virt_supported.load(Ordering::SeqCst))
        }

        fn set_nested_virt(&self, ctx: CtxId, enabled: bool) -> Result<(), TateruError> {
            self.record(EngineCall::SetNestedVirt {
                ctx: ctx.0,
                enabled,
            });
            Ok(())
        }

        fn set_smbios_oem_strings(
            &self,
            ctx: CtxId,
            strings: &[&str],
        ) -> Result<(), TateruError> {
            self.record(EngineCall::SetSmbiosOemStrings {
                ctx: ctx.0,
                count: strings.len(),
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    #[test]
    fn mock_create_ctx() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        assert_eq!(ctx.raw(), 0);
        let ctx2 = engine.create_ctx().unwrap();
        assert_eq!(ctx2.raw(), 1);
    }

    #[test]
    fn mock_create_ctx_failure() {
        let engine = MockEngine::new();
        engine.fail_create_ctx.store(true, Ordering::SeqCst);
        let err = engine.create_ctx().unwrap_err();
        assert!(matches!(err, TateruError::Ffi { .. }));
    }

    #[test]
    fn mock_set_vm_config() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let vcpus = VcpuCount::new(6).unwrap();
        let memory = MemoryMib::new(8192).unwrap();
        engine.set_vm_config(ctx, vcpus, memory).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(
            calls[1],
            EngineCall::SetVmConfig {
                ctx: 0,
                vcpus: 6,
                memory_mib: 8192,
            }
        ));
    }

    #[test]
    fn mock_set_vm_config_failure() {
        let engine = MockEngine::new();
        engine.fail_set_vm_config.store(true, Ordering::SeqCst);
        let ctx = engine.create_ctx().unwrap();
        let vcpus = VcpuCount::new(6).unwrap();
        let memory = MemoryMib::new(8192).unwrap();
        let err = engine.set_vm_config(ctx, vcpus, memory).unwrap_err();
        assert!(matches!(err, TateruError::Ffi { .. }));
    }

    #[test]
    fn mock_add_disk() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let disk = DiskConfig {
            path: PathBuf::from("/test.qcow2"),
            format: crate::devices::DiskFormat::Qcow2,
            read_only: false,
        };
        engine.add_disk(ctx, &disk, 0).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(
            calls[1],
            EngineCall::AddDisk { ctx: 0, index: 0 }
        ));
    }

    #[test]
    fn mock_add_disk_failure() {
        let engine = MockEngine::new();
        engine.fail_add_disk.store(true, Ordering::SeqCst);
        let ctx = engine.create_ctx().unwrap();
        let disk = DiskConfig {
            path: PathBuf::from("/test.qcow2"),
            format: crate::devices::DiskFormat::Qcow2,
            read_only: false,
        };
        let err = engine.add_disk(ctx, &disk, 0).unwrap_err();
        assert!(matches!(err, TateruError::Ffi { .. }));
    }

    #[test]
    fn mock_add_virtiofs() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let mount = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "data".into(),
        };
        engine.add_virtiofs(ctx, &mount).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(
            &calls[1],
            EngineCall::AddVirtiofs { ctx: 0, tag } if tag == "data"
        ));
    }

    #[test]
    fn mock_add_vsock_port() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let port = VsockPort {
            guest_port: crate::types::GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/vsock.sock"),
        };
        engine.add_vsock_port(ctx, &port).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(
            calls[1],
            EngineCall::AddVsockPort {
                ctx: 0,
                guest_port: 22,
            }
        ));
    }

    #[test]
    fn mock_get_shutdown_eventfd() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let fd = engine.get_shutdown_eventfd(ctx).unwrap();
        // Returns a real pipe fd (>= 0), not the old dummy value
        assert!(fd >= 0);
        // Clean up the fd
        unsafe { libc::close(fd) };
    }

    #[test]
    fn mock_start_enter() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        engine.start_enter(ctx).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(calls[1], EngineCall::StartEnter { ctx: 0 }));
    }

    #[test]
    fn mock_start_enter_failure() {
        let engine = MockEngine::new();
        engine.fail_start_enter.store(true, Ordering::SeqCst);
        let ctx = engine.create_ctx().unwrap();
        let err = engine.start_enter(ctx).unwrap_err();
        assert!(matches!(err, TateruError::Ffi { .. }));
    }

    #[test]
    fn mock_check_nested_virt() {
        let engine = MockEngine::new();
        assert!(engine.check_nested_virt().unwrap());

        engine
            .nested_virt_supported
            .store(false, Ordering::SeqCst);
        assert!(!engine.check_nested_virt().unwrap());
    }

    #[test]
    fn mock_set_log_level() {
        let engine = MockEngine::new();
        engine.set_log_level(LogLevel::Debug).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(
            calls[0],
            EngineCall::SetLogLevel { level: 4 }
        ));
    }

    #[test]
    fn mock_records_all_calls_in_order() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let vcpus = VcpuCount::new(2).unwrap();
        let memory = MemoryMib::new(1024).unwrap();
        engine.set_vm_config(ctx, vcpus, memory).unwrap();
        engine.check_nested_virt().unwrap();

        let calls = engine.recorded_calls();
        assert_eq!(calls.len(), 3);
        assert!(matches!(calls[0], EngineCall::CreateCtx));
        assert!(matches!(calls[1], EngineCall::SetVmConfig { .. }));
        assert!(matches!(calls[2], EngineCall::CheckNestedVirt));
    }

    #[test]
    fn mock_default() {
        let engine = MockEngine::default();
        assert!(engine.recorded_calls().is_empty());
    }

    #[test]
    fn mock_set_smbios_oem_strings() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        engine
            .set_smbios_oem_strings(ctx, &["key1=val1", "key2=val2"])
            .unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(
            calls[1],
            EngineCall::SetSmbiosOemStrings { ctx: 0, count: 2 }
        ));
    }

    #[test]
    fn mock_set_nested_virt() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        engine.set_nested_virt(ctx, true).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(
            calls[1],
            EngineCall::SetNestedVirt {
                ctx: 0,
                enabled: true
            }
        ));
    }

    #[test]
    fn libkrun_engine_is_zero_sized() {
        assert_eq!(std::mem::size_of::<LibkrunEngine>(), 0);
    }

    #[test]
    fn mock_set_console_output() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let console = ConsoleConfig {
            log_path: PathBuf::from("/var/log/console.log"),
        };
        engine.set_console_output(ctx, &console).unwrap();

        let calls = engine.recorded_calls();
        assert!(matches!(calls[1], EngineCall::SetConsoleOutput { ctx: 0 }));
    }

    #[test]
    fn mock_get_shutdown_eventfd_failure() {
        let engine = MockEngine::new();
        engine
            .fail_get_shutdown_eventfd
            .store(true, Ordering::SeqCst);
        let ctx = engine.create_ctx().unwrap();
        let err = engine.get_shutdown_eventfd(ctx).unwrap_err();
        assert!(matches!(err, TateruError::Ffi { .. }));
    }

    #[test]
    fn mock_engine_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MockEngine>();
    }

    #[test]
    fn mock_engine_ctx_ids_increment() {
        let engine = MockEngine::new();
        for expected in 0..5 {
            let ctx = engine.create_ctx().unwrap();
            assert_eq!(ctx.raw(), expected);
        }
    }

    #[test]
    fn engine_call_clone() {
        let call = EngineCall::SetVmConfig {
            ctx: 0,
            vcpus: 4,
            memory_mib: 8192,
        };
        let cloned = call.clone();
        assert_eq!(call, cloned);
    }

    #[test]
    fn engine_call_debug() {
        let call = EngineCall::CreateCtx;
        let debug = format!("{call:?}");
        assert!(debug.contains("CreateCtx"));
    }

    #[test]
    fn engine_call_equality_different_variants() {
        assert_ne!(EngineCall::CreateCtx, EngineCall::CheckNestedVirt);
    }

    #[test]
    fn engine_call_set_vm_config_inequality() {
        let a = EngineCall::SetVmConfig { ctx: 0, vcpus: 4, memory_mib: 8192 };
        let b = EngineCall::SetVmConfig { ctx: 0, vcpus: 8, memory_mib: 8192 };
        assert_ne!(a, b);
    }

    #[test]
    fn engine_call_add_disk_inequality() {
        let a = EngineCall::AddDisk { ctx: 0, index: 0 };
        let b = EngineCall::AddDisk { ctx: 0, index: 1 };
        assert_ne!(a, b);
    }

    #[test]
    fn mock_full_lifecycle_records_all_calls() {
        let engine = MockEngine::new();
        let ctx = engine.create_ctx().unwrap();
        let vcpus = VcpuCount::new(4).unwrap();
        let memory = MemoryMib::new(4096).unwrap();
        engine.set_vm_config(ctx, vcpus, memory).unwrap();

        let disk = DiskConfig {
            path: PathBuf::from("/test.qcow2"),
            format: crate::devices::DiskFormat::Qcow2,
            read_only: false,
        };
        engine.add_disk(ctx, &disk, 0).unwrap();

        let mount = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "data".into(),
        };
        engine.add_virtiofs(ctx, &mount).unwrap();

        let port = VsockPort {
            guest_port: crate::types::GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/vsock.sock"),
        };
        engine.add_vsock_port(ctx, &port).unwrap();

        let console = ConsoleConfig {
            log_path: PathBuf::from("/var/log/console.log"),
        };
        engine.set_console_output(ctx, &console).unwrap();

        engine
            .set_smbios_oem_strings(ctx, &["key=val"])
            .unwrap();
        engine.set_nested_virt(ctx, true).unwrap();

        let fd = engine.get_shutdown_eventfd(ctx).unwrap();
        assert!(fd >= 0);
        unsafe { libc::close(fd) };

        engine.start_enter(ctx).unwrap();

        let calls = engine.recorded_calls();
        assert_eq!(calls.len(), 10);
        assert!(matches!(calls[0], EngineCall::CreateCtx));
        assert!(matches!(calls[1], EngineCall::SetVmConfig { .. }));
        assert!(matches!(calls[2], EngineCall::AddDisk { .. }));
        assert!(matches!(calls[3], EngineCall::AddVirtiofs { .. }));
        assert!(matches!(calls[4], EngineCall::AddVsockPort { .. }));
        assert!(matches!(calls[5], EngineCall::SetConsoleOutput { .. }));
        assert!(matches!(calls[6], EngineCall::SetSmbiosOemStrings { .. }));
        assert!(matches!(calls[7], EngineCall::SetNestedVirt { .. }));
        assert!(matches!(
            calls[8],
            EngineCall::GetShutdownEventfd { .. }
        ));
        assert!(matches!(calls[9], EngineCall::StartEnter { .. }));
    }
}
