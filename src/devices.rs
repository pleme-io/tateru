//! Typed device configuration for VM peripherals.
//!
//! These are pure data structures — no FFI calls. The [`VmEngine`](crate::engine::VmEngine)
//! trait implementation is responsible for translating these into FFI calls.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::types::GuestPort;

/// Disk image format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiskFormat {
    Raw,
    Qcow2,
}

impl DiskFormat {
    /// Convert to the libkrun FFI constant.
    #[inline]
    pub(crate) fn to_ffi(self) -> u32 {
        match self {
            Self::Raw => crate::ffi::KRUN_DISK_FORMAT_RAW,
            Self::Qcow2 => crate::ffi::KRUN_DISK_FORMAT_QCOW2,
        }
    }
}

/// A disk image attached to the VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskConfig {
    /// Path to the disk image file.
    pub path: PathBuf,
    /// Image format.
    pub format: DiskFormat,
    /// Mount read-only.
    pub read_only: bool,
}

/// A host directory shared with the guest via virtiofs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtioFsMount {
    /// Path on the host to share.
    pub host_path: PathBuf,
    /// Mount tag visible inside the guest.
    pub mount_tag: String,
}

/// A vsock port mapping backed by a Unix socket on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VsockPort {
    /// Port number visible inside the guest.
    pub guest_port: GuestPort,
    /// Path to the Unix socket on the host.
    pub host_socket: PathBuf,
}

/// Console output redirection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleConfig {
    /// Path to the console log file.
    pub log_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_format_ffi_values() {
        assert_eq!(DiskFormat::Raw.to_ffi(), 0);
        assert_eq!(DiskFormat::Qcow2.to_ffi(), 1);
    }

    #[test]
    fn disk_config_construction() {
        let disk = DiskConfig {
            path: PathBuf::from("/var/lib/vm/guest.qcow2"),
            format: DiskFormat::Qcow2,
            read_only: false,
        };
        assert_eq!(disk.path, PathBuf::from("/var/lib/vm/guest.qcow2"));
        assert_eq!(disk.format, DiskFormat::Qcow2);
        assert!(!disk.read_only);
    }

    #[test]
    fn disk_config_read_only() {
        let disk = DiskConfig {
            path: PathBuf::from("/images/base.raw"),
            format: DiskFormat::Raw,
            read_only: true,
        };
        assert!(disk.read_only);
        assert_eq!(disk.format, DiskFormat::Raw);
    }

    #[test]
    fn virtiofs_mount_construction() {
        let mount = VirtioFsMount {
            host_path: PathBuf::from("/Library/Apple/usr/libexec/oah"),
            mount_tag: "rosetta".into(),
        };
        assert_eq!(mount.mount_tag, "rosetta");
    }

    #[test]
    fn vsock_port_construction() {
        let port = VsockPort {
            guest_port: GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/vsock-22.sock"),
        };
        assert_eq!(port.guest_port.raw(), 22);
    }

    #[test]
    fn console_config_construction() {
        let console = ConsoleConfig {
            log_path: PathBuf::from("/var/log/vm-console.log"),
        };
        assert_eq!(console.log_path, PathBuf::from("/var/log/vm-console.log"));
    }

    #[test]
    fn disk_config_clone() {
        let a = DiskConfig {
            path: PathBuf::from("/a.qcow2"),
            format: DiskFormat::Qcow2,
            read_only: false,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn disk_config_serde_roundtrip() {
        let disk = DiskConfig {
            path: PathBuf::from("/var/lib/vm/guest.qcow2"),
            format: DiskFormat::Qcow2,
            read_only: false,
        };
        let json = serde_json::to_string(&disk).unwrap();
        let disk2: DiskConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(disk, disk2);
    }

    #[test]
    fn virtiofs_serde_roundtrip() {
        let mount = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "data".into(),
        };
        let json = serde_json::to_string(&mount).unwrap();
        let mount2: VirtioFsMount = serde_json::from_str(&json).unwrap();
        assert_eq!(mount, mount2);
    }

    #[test]
    fn vsock_serde_roundtrip() {
        let port = VsockPort {
            guest_port: GuestPort::new(8080).unwrap(),
            host_socket: PathBuf::from("/tmp/vsock.sock"),
        };
        let json = serde_json::to_string(&port).unwrap();
        let port2: VsockPort = serde_json::from_str(&json).unwrap();
        assert_eq!(port, port2);
    }

    #[test]
    fn console_serde_roundtrip() {
        let console = ConsoleConfig {
            log_path: PathBuf::from("/var/log/console.log"),
        };
        let json = serde_json::to_string(&console).unwrap();
        let console2: ConsoleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(console, console2);
    }

    #[test]
    fn disk_format_equality() {
        assert_eq!(DiskFormat::Raw, DiskFormat::Raw);
        assert_eq!(DiskFormat::Qcow2, DiskFormat::Qcow2);
        assert_ne!(DiskFormat::Raw, DiskFormat::Qcow2);
    }

    // -- DiskFormat serde edge cases --
    // Catches bugs where rename_all = "lowercase" doesn't apply consistently.

    #[test]
    fn disk_format_serde_raw_string() {
        let json = "\"raw\"";
        let f: DiskFormat = serde_json::from_str(json).unwrap();
        assert_eq!(f, DiskFormat::Raw);
    }

    #[test]
    fn disk_format_serde_qcow2_string() {
        let json = "\"qcow2\"";
        let f: DiskFormat = serde_json::from_str(json).unwrap();
        assert_eq!(f, DiskFormat::Qcow2);
    }

    #[test]
    fn disk_format_serde_uppercase_rejected() {
        let json = "\"RAW\"";
        let result: Result<DiskFormat, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn disk_format_serde_invalid_value() {
        let json = "\"vmdk\"";
        let result: Result<DiskFormat, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn disk_format_serde_roundtrip_raw() {
        let json = serde_json::to_string(&DiskFormat::Raw).unwrap();
        assert_eq!(json, "\"raw\"");
        let f: DiskFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(f, DiskFormat::Raw);
    }

    #[test]
    fn disk_format_serde_roundtrip_qcow2() {
        let json = serde_json::to_string(&DiskFormat::Qcow2).unwrap();
        assert_eq!(json, "\"qcow2\"");
        let f: DiskFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(f, DiskFormat::Qcow2);
    }

    // -- DiskConfig serde with read_only = true --

    #[test]
    fn disk_config_serde_read_only() {
        let disk = DiskConfig {
            path: PathBuf::from("/test.raw"),
            format: DiskFormat::Raw,
            read_only: true,
        };
        let json = serde_json::to_string(&disk).unwrap();
        let disk2: DiskConfig = serde_json::from_str(&json).unwrap();
        assert!(disk2.read_only);
        assert_eq!(disk2.format, DiskFormat::Raw);
    }

    // -- VsockPort equality --

    #[test]
    fn vsock_port_equality() {
        let a = VsockPort {
            guest_port: GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/a.sock"),
        };
        let b = VsockPort {
            guest_port: GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/a.sock"),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn vsock_port_inequality_port() {
        let a = VsockPort {
            guest_port: GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/a.sock"),
        };
        let b = VsockPort {
            guest_port: GuestPort::new(80).unwrap(),
            host_socket: PathBuf::from("/tmp/a.sock"),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn vsock_port_inequality_socket() {
        let a = VsockPort {
            guest_port: GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/a.sock"),
        };
        let b = VsockPort {
            guest_port: GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/b.sock"),
        };
        assert_ne!(a, b);
    }

    // -- ConsoleConfig clone and equality --

    #[test]
    fn console_config_equality() {
        let a = ConsoleConfig {
            log_path: PathBuf::from("/var/log/a.log"),
        };
        let b = ConsoleConfig {
            log_path: PathBuf::from("/var/log/a.log"),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn console_config_inequality() {
        let a = ConsoleConfig {
            log_path: PathBuf::from("/var/log/a.log"),
        };
        let b = ConsoleConfig {
            log_path: PathBuf::from("/var/log/b.log"),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn console_config_clone() {
        let a = ConsoleConfig {
            log_path: PathBuf::from("/var/log/test.log"),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // -- VirtioFsMount equality and clone --

    #[test]
    fn virtiofs_mount_equality() {
        let a = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "data".into(),
        };
        let b = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "data".into(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn virtiofs_mount_inequality() {
        let a = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "data".into(),
        };
        let b = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "other".into(),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn virtiofs_mount_clone() {
        let a = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "data".into(),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // -- DiskFormat copy semantics --

    #[test]
    fn disk_format_copy() {
        let a = DiskFormat::Raw;
        let b = a;
        let c = a;
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    // -- Debug format tests --

    #[test]
    fn disk_config_debug() {
        let d = DiskConfig {
            path: PathBuf::from("/test.qcow2"),
            format: DiskFormat::Qcow2,
            read_only: false,
        };
        let dbg = format!("{d:?}");
        assert!(dbg.contains("DiskConfig"));
        assert!(dbg.contains("test.qcow2"));
    }

    #[test]
    fn virtiofs_debug() {
        let m = VirtioFsMount {
            host_path: PathBuf::from("/shared"),
            mount_tag: "tag".into(),
        };
        let dbg = format!("{m:?}");
        assert!(dbg.contains("VirtioFsMount"));
    }

    #[test]
    fn vsock_port_debug() {
        let p = VsockPort {
            guest_port: GuestPort::new(22).unwrap(),
            host_socket: PathBuf::from("/tmp/test.sock"),
        };
        let dbg = format!("{p:?}");
        assert!(dbg.contains("VsockPort"));
    }

    #[test]
    fn console_config_debug() {
        let c = ConsoleConfig {
            log_path: PathBuf::from("/var/log/test.log"),
        };
        let dbg = format!("{c:?}");
        assert!(dbg.contains("ConsoleConfig"));
    }
}
