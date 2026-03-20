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

pub mod devices;
pub mod engine;
pub mod error;
pub mod types;

pub(crate) mod bridge;
pub(crate) mod ffi;
pub(crate) mod shutdown;
pub mod vm;

#[cfg(feature = "config")]
pub mod config;

// Re-exports for ergonomic use
pub use devices::{ConsoleConfig, DiskConfig, DiskFormat, VirtioFsMount, VsockPort};
pub use engine::{LibkrunEngine, VmEngine};
pub use error::TateruError;
pub use types::{CtxId, GuestPort, LogLevel, MemoryMib, VcpuCount};
pub use vm::{Vm, VmBuilder, VmHandle};

#[cfg(any(test, feature = "testing"))]
pub use engine::mock;
