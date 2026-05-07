//! Build a `VZVirtualMachineConfiguration` from a `VzCtxState`.
//!
//! This is M1a — assembly only. M1b runs the resulting configuration
//! through `VZVirtualMachine.startWithCompletionHandler:` on a
//! dispatch queue. M1a is reviewable in isolation: types line up,
//! cargo check is green, the surface mirrors libkrun's per-context
//! ops the same way `VzCtxState` mirrors libkrun's incremental setup.
//!
//! # Substrate alignment
//!
//! Each `VzCtxState` field maps to exactly one VZ configuration node;
//! the unidirectional flow (state → configuration) is the typed
//! morphism between tateru's engine-agnostic surface and Apple's
//! declarative VZ shape. No state mutation here — pure construction.

use std::path::Path;

use objc2::rc::Retained;
use objc2::{AllocAnyThread, ClassType};
use objc2_foundation::{NSArray, NSString, NSURL};
use objc2_virtualization::{
    VZDiskImageStorageDeviceAttachment, VZEFIBootLoader, VZStorageDeviceConfiguration,
    VZVirtioBlockDeviceConfiguration, VZVirtualMachineConfiguration,
};

use crate::error::TateruError;
use crate::vz::state::VzCtxState;

/// Build a complete `VZVirtualMachineConfiguration` from collected
/// per-context ops.
///
/// Validation is **not** called here — the caller decides when (M1b
/// will validate before starting).
pub(crate) fn build_configuration(
    state: &VzCtxState,
) -> Result<Retained<VZVirtualMachineConfiguration>, TateruError> {
    let vcpus = state
        .vcpus
        .ok_or_else(|| TateruError::InvalidConfig("vz: vcpus not set".into()))?;
    let memory = state
        .memory
        .ok_or_else(|| TateruError::InvalidConfig("vz: memory not set".into()))?;

    if state.disks.is_empty() {
        return Err(TateruError::InvalidConfig(
            "vz: at least one disk is required (boot media)".into(),
        ));
    }

    // Safety: every VZ ObjC method is `unsafe fn` because it crosses
    // the FFI line. We're calling them with valid retained owners
    // and primitive types as Apple's headers require.
    unsafe {
        let cfg = VZVirtualMachineConfiguration::new();
        cfg.setCPUCount(vcpus.raw() as usize);
        cfg.setMemorySize(u64::from(memory.raw()) * 1024 * 1024);

        // EFI bootloader — the cluster's NixOS guest has systemd-boot
        // installed on /boot via amazon-image; VZ's EFI loader walks
        // the boot manager entries automatically. M2 will add a
        // VZEFIVariableStore so the boot order persists across runs.
        let boot_loader = VZEFIBootLoader::new();
        cfg.setBootLoader(Some(&boot_loader));

        // Disks → VZDiskImageStorageDeviceAttachment → VZVirtioBlockDeviceConfiguration.
        let mut storage_devs: Vec<Retained<VZStorageDeviceConfiguration>> =
            Vec::with_capacity(state.disks.len());
        for disk in &state.disks {
            let attachment = make_disk_attachment(&disk.path, disk.read_only)?;
            let block = VZVirtioBlockDeviceConfiguration::initWithAttachment(
                VZVirtioBlockDeviceConfiguration::alloc(),
                attachment.as_super(),
            );
            storage_devs.push(Retained::cast_unchecked(block));
        }
        let storage_array = NSArray::from_retained_slice(&storage_devs);
        cfg.setStorageDevices(&storage_array);

        Ok(cfg)
    }
}

/// Wrap a host-path disk image as a `VZDiskImageStorageDeviceAttachment`.
unsafe fn make_disk_attachment(
    path: &Path,
    read_only: bool,
) -> Result<Retained<VZDiskImageStorageDeviceAttachment>, TateruError> {
    let path_str = path.to_str().ok_or_else(|| {
        TateruError::InvalidConfig(format!("vz: non-utf8 disk path: {}", path.display()))
    })?;
    let ns_path = NSString::from_str(path_str);
    let url = NSURL::fileURLWithPath(&ns_path);

    let attachment = unsafe {
        VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
            VZDiskImageStorageDeviceAttachment::alloc(),
            &url,
            read_only,
        )
    };
    attachment.map_err(|err| {
        TateruError::InvalidConfig(format!(
            "vz: disk attachment failed for {}: {}",
            path.display(),
            err
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::devices::{DiskConfig, DiskFormat};
    use crate::types::{MemoryMib, VcpuCount};
    use crate::vz::state::VzCtxState;

    fn minimal_state(disk_path: PathBuf) -> VzCtxState {
        VzCtxState {
            vcpus: Some(VcpuCount::new(2).unwrap()),
            memory: Some(MemoryMib::new(1024).unwrap()),
            disks: vec![DiskConfig {
                path: disk_path,
                format: DiskFormat::Qcow2,
                read_only: false,
            }],
            ..VzCtxState::default()
        }
    }

    #[test]
    fn missing_vcpus_errors() {
        let mut s = VzCtxState::default();
        s.memory = Some(MemoryMib::new(1024).unwrap());
        let err = build_configuration(&s).unwrap_err();
        assert!(matches!(err, TateruError::InvalidConfig(m) if m.contains("vcpus")));
    }

    #[test]
    fn missing_disk_errors() {
        let s = VzCtxState {
            vcpus: Some(VcpuCount::new(2).unwrap()),
            memory: Some(MemoryMib::new(1024).unwrap()),
            ..VzCtxState::default()
        };
        let err = build_configuration(&s).unwrap_err();
        assert!(matches!(err, TateruError::InvalidConfig(m) if m.contains("disk")));
    }

    #[test]
    fn disk_attachment_fails_on_missing_path() {
        let s = minimal_state(PathBuf::from("/this/path/definitely/does/not/exist.qcow2"));
        // VZDiskImageStorageDeviceAttachment.initWithURL: returns
        // an NSError on missing files. Verifies our error wrapping
        // works without spinning up an actual VM.
        let err = build_configuration(&s).unwrap_err();
        assert!(matches!(err, TateruError::InvalidConfig(m) if m.contains("disk attachment")));
    }
}
