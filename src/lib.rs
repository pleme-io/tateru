//! Tateru — direct libkrun FFI control for macOS VMs.
//!
//! This library provides a safe, strongly-typed Rust API over the libkrun-efi
//! C library for launching and managing lightweight Linux VMs on Apple Silicon
//! via Apple Hypervisor.framework.
//!
//! # Architecture
//!
//! All libkrun FFI calls are abstracted behind the [`VmEngine`] trait,
//! enabling full mockability for testing. Strong newtypes ([`CtxId`],
//! [`VcpuCount`], [`MemoryMib`], [`GuestPort`]) wrap raw FFI values to
//! prevent misuse.
//!
//! # Example
//!
//! ```ignore
//! use tateru::{Vm, LibkrunEngine, DiskConfig, DiskFormat, VirtioFsMount};
//!
//! let mut handle = Vm::builder(LibkrunEngine::new())
//!     .cpus(6)?
//!     .memory("8GiB")?
//!     .disk(DiskConfig { path: "guest.qcow2".into(), format: DiskFormat::Qcow2, read_only: false })
//!     .virtiofs(VirtioFsMount { host_path: "/shared".into(), mount_tag: "data".into() })
//!     .vsock_bridge(31122, 22)?
//!     .launch()
//!     .await?;
//!
//! handle.stop()?;
//! ```

pub mod bridge;
pub mod devices;
pub mod engine;
pub mod error;
pub mod shutdown;
pub mod types;
pub mod vm;

pub(crate) mod ffi;

#[cfg(feature = "config")]
pub mod config;

// Re-exports for ergonomic use
pub use bridge::{BridgeConfig, BridgeHandle};
pub use devices::{ConsoleConfig, DiskConfig, DiskFormat, VirtioFsMount, VsockPort};
pub use engine::{LibkrunEngine, VmEngine};
pub use error::TateruError;
pub use shutdown::Shutdown;
pub use types::{CtxId, GuestPort, LogLevel, MemoryMib, VcpuCount};
pub use vm::{BridgeSpawner, TokioBridgeSpawner, Vm, VmBuilder, VmControl, VmHandle};

#[cfg(any(test, feature = "testing"))]
pub mod mock {
    //! Mock types for testing without real libkrun.
    //!
    //! Re-exports all mock implementations from their respective modules.
    pub use crate::engine::mock::{EngineCall, MockEngine};
    pub use crate::shutdown::MockShutdown;
    pub use crate::vm::{MockBridgeSpawner, MockVmControl};
}
