//! Per-context VZ state — collected lazily until `start_enter`
//! materializes a `VZVirtualMachineConfiguration`.
//!
//! libkrun's C API is imperative: `add_disk` / `add_virtiofs` /
//! `set_vm_config` mutate context state in libkrun's address space,
//! then `krun_start_enter` launches. VZ is declarative: build a
//! complete `VZVirtualMachineConfiguration`, call `validate()`, then
//! `start()`. The two shapes don't compose at the FFI line, so VZ
//! collects every per-context call into a Rust-side struct and only
//! crosses into Objective-C land at launch time.
//!
//! This keeps the `VmEngine` trait shape identical for both engines.
#![allow(dead_code)]

use std::path::PathBuf;

use crate::devices::{ConsoleConfig, DiskConfig, VirtioFsMount, VsockPort};
use crate::types::{MemoryMib, VcpuCount};

/// Mutable per-context state. Wrapped in `Mutex` inside `VzEngine`.
#[derive(Debug, Default)]
pub(crate) struct VzCtxState {
    /// Set by `set_vm_config`.
    pub(crate) vcpus: Option<VcpuCount>,
    /// Set by `set_vm_config`. Stored as MiB to match the trait
    /// surface; converted to bytes (MiB × 1024 × 1024) when the VZ
    /// configuration is materialized.
    pub(crate) memory: Option<MemoryMib>,

    /// Disks in declaration order; index 0 is conventionally the
    /// boot/root disk.
    pub(crate) disks: Vec<DiskConfig>,

    /// virtiofs shares. Tag `"rosetta"` is intercepted at launch time
    /// and materialized as `VZLinuxRosettaDirectoryShare` instead of
    /// the generic `VZSharedDirectory` — that's what flips
    /// `cpu.rosettaModeEnabled` and unlocks x86_64 translation.
    pub(crate) virtiofs: Vec<VirtioFsMount>,

    /// vsock listen-mode entries: each pair `(guest_port, host_socket_path)`
    /// becomes one `VZVirtioSocketListener` bound to a Unix socket on
    /// the host. Listen-mode means the host opens the socket and
    /// `accept()`s; the guest connects out via vsock — same shape as
    /// libkrun's `krun_add_vsock_port2(..., listen=true)`.
    pub(crate) vsock_ports: Vec<VsockPort>,

    /// Console redirection target. None = inherit stdio (rare in
    /// production, useful for tests).
    pub(crate) console: Option<ConsoleConfig>,

    /// Optional initial kernel cmdline override. VZ uses
    /// `VZLinuxBootLoader` with explicit kernel + initramfs paths
    /// when set, otherwise relies on `VZEFIBootLoader` + the disk's
    /// own bootloader.
    pub(crate) kernel_cmdline: Option<String>,
    pub(crate) kernel_path: Option<PathBuf>,
    pub(crate) initramfs_path: Option<PathBuf>,
}
