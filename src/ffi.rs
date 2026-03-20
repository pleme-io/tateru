//! Raw FFI bindings to libkrun-efi.
//!
//! All functions are `pub(crate)` — consumers use the safe `Vm` builder API.

use std::ffi::CString;
use std::path::Path;

use crate::error::TateruError;

// ---------------------------------------------------------------------------
// libkrun C API declarations
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn krun_create_ctx() -> i32;
    fn krun_set_vm_config(ctx_id: u32, num_vcpus: u8, ram_mib: u32) -> i32;
    fn krun_add_disk2(
        ctx_id: u32,
        block_id: *const std::ffi::c_char,
        disk_path: *const std::ffi::c_char,
        disk_format: u32,
        read_only: bool,
    ) -> i32;
    fn krun_add_virtiofs(
        ctx_id: u32,
        tag: *const std::ffi::c_char,
        path: *const std::ffi::c_char,
    ) -> i32;
    fn krun_add_vsock_port(
        ctx_id: u32,
        port: u32,
        filepath: *const std::ffi::c_char,
    ) -> i32;
    fn krun_set_console_output(ctx_id: u32, filepath: *const std::ffi::c_char) -> i32;
    fn krun_start_enter(ctx_id: u32) -> i32;
    fn krun_get_shutdown_eventfd(ctx_id: u32) -> i32;
    fn krun_set_log_level(level: u32) -> i32;
    fn krun_set_smbios_oem_strings(
        ctx_id: u32,
        oem_strings: *const *const std::ffi::c_char,
    ) -> i32;
    fn krun_set_nested_virt(ctx_id: u32, enabled: bool) -> i32;
    fn krun_check_nested_virt() -> i32;
}

// ---------------------------------------------------------------------------
// Disk format constants (from libkrun.h)
// ---------------------------------------------------------------------------

pub(crate) const KRUN_DISK_FORMAT_RAW: u32 = 0;
pub(crate) const KRUN_DISK_FORMAT_QCOW2: u32 = 1;

// ---------------------------------------------------------------------------
// Helper: convert Path to CString
// ---------------------------------------------------------------------------

pub(crate) fn path_to_cstring(path: &Path) -> Result<CString, TateruError> {
    let s = path
        .to_str()
        .ok_or_else(|| TateruError::InvalidPath(path.to_path_buf()))?;
    CString::new(s).map_err(|_| TateruError::InvalidPath(path.to_path_buf()))
}

fn str_to_cstring(s: &str) -> Result<CString, TateruError> {
    CString::new(s).map_err(|_| TateruError::InvalidConfig(format!("string contains NUL: {s}")))
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

fn check(function: &'static str, ret: i32) -> Result<(), TateruError> {
    if ret < 0 {
        Err(TateruError::Ffi { function, code: ret })
    } else {
        Ok(())
    }
}

/// Create a new VM context. Returns the context ID.
pub(crate) fn create_ctx() -> Result<u32, TateruError> {
    let ret = unsafe { krun_create_ctx() };
    if ret < 0 {
        Err(TateruError::Ffi {
            function: "krun_create_ctx",
            code: ret,
        })
    } else {
        #[allow(clippy::cast_sign_loss)]
        Ok(ret as u32)
    }
}

/// Configure vCPUs and memory.
pub(crate) fn set_vm_config(ctx_id: u32, vcpus: u8, ram_mib: u32) -> Result<(), TateruError> {
    let ret = unsafe { krun_set_vm_config(ctx_id, vcpus, ram_mib) };
    check("krun_set_vm_config", ret)
}

/// Add a disk image to the VM.
pub(crate) fn add_disk(
    ctx_id: u32,
    block_id: &str,
    disk_path: &Path,
    format: u32,
    read_only: bool,
) -> Result<(), TateruError> {
    let c_block_id = str_to_cstring(block_id)?;
    let c_path = path_to_cstring(disk_path)?;
    let ret = unsafe {
        krun_add_disk2(
            ctx_id,
            c_block_id.as_ptr(),
            c_path.as_ptr(),
            format,
            read_only,
        )
    };
    check("krun_add_disk2", ret)
}

/// Add a virtiofs shared directory.
pub(crate) fn add_virtiofs(ctx_id: u32, tag: &str, path: &Path) -> Result<(), TateruError> {
    let c_tag = str_to_cstring(tag)?;
    let c_path = path_to_cstring(path)?;
    let ret = unsafe { krun_add_virtiofs(ctx_id, c_tag.as_ptr(), c_path.as_ptr()) };
    check("krun_add_virtiofs", ret)
}

/// Register a vsock port backed by a Unix socket.
pub(crate) fn add_vsock_port(
    ctx_id: u32,
    port: u32,
    socket_path: &Path,
) -> Result<(), TateruError> {
    let c_path = path_to_cstring(socket_path)?;
    let ret = unsafe { krun_add_vsock_port(ctx_id, port, c_path.as_ptr()) };
    check("krun_add_vsock_port", ret)
}

/// Redirect console output to a file.
pub(crate) fn set_console_output(ctx_id: u32, path: &Path) -> Result<(), TateruError> {
    let c_path = path_to_cstring(path)?;
    let ret = unsafe { krun_set_console_output(ctx_id, c_path.as_ptr()) };
    check("krun_set_console_output", ret)
}

/// Start the VM. **Blocks the calling thread forever** — the thread becomes the VM.
///
/// Only returns on error before the VM starts.
pub(crate) fn start_enter(ctx_id: u32) -> Result<(), TateruError> {
    let ret = unsafe { krun_start_enter(ctx_id) };
    // start_enter only returns on error
    Err(TateruError::Ffi {
        function: "krun_start_enter",
        code: ret,
    })
}

/// Get the shutdown eventfd. Must be called before `start_enter`.
///
/// Returns the raw file descriptor.
pub(crate) fn get_shutdown_eventfd(ctx_id: u32) -> Result<i32, TateruError> {
    let ret = unsafe { krun_get_shutdown_eventfd(ctx_id) };
    if ret < 0 {
        Err(TateruError::Ffi {
            function: "krun_get_shutdown_eventfd",
            code: ret,
        })
    } else {
        Ok(ret)
    }
}

/// Set libkrun log level (0=Off .. 5=Trace).
pub(crate) fn set_log_level(level: u32) -> Result<(), TateruError> {
    let ret = unsafe { krun_set_log_level(level) };
    check("krun_set_log_level", ret)
}

/// Set SMBIOS OEM strings.
pub(crate) fn set_smbios_oem_strings(
    ctx_id: u32,
    strings: &[&str],
) -> Result<(), TateruError> {
    let c_strings: Vec<CString> = strings
        .iter()
        .map(|s| str_to_cstring(s))
        .collect::<Result<_, _>>()?;

    let mut ptrs: Vec<*const std::ffi::c_char> =
        c_strings.iter().map(|cs| cs.as_ptr()).collect();
    ptrs.push(std::ptr::null()); // NULL terminator

    let ret = unsafe { krun_set_smbios_oem_strings(ctx_id, ptrs.as_ptr()) };
    check("krun_set_smbios_oem_strings", ret)
}

/// Enable or disable nested virtualization (macOS only).
pub(crate) fn set_nested_virt(ctx_id: u32, enabled: bool) -> Result<(), TateruError> {
    let ret = unsafe { krun_set_nested_virt(ctx_id, enabled) };
    check("krun_set_nested_virt", ret)
}

/// Check if nested virtualization is supported.
///
/// Returns `true` if supported, `false` if not.
pub(crate) fn check_nested_virt() -> Result<bool, TateruError> {
    let ret = unsafe { krun_check_nested_virt() };
    if ret < 0 {
        Err(TateruError::Ffi {
            function: "krun_check_nested_virt",
            code: ret,
        })
    } else {
        Ok(ret == 1)
    }
}
