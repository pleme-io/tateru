//! Optional shikumi-based configuration (feature-gated behind `config`).
//!
//! Config file at `~/.config/tateru/tateru.yaml`.
//!
//! Most consumers (e.g. libkrun-builder) use the builder API directly.
//! This module is for standalone use.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::devices::{DiskConfig, DiskFormat, VirtioFsMount};
use crate::error::TateruError;
use crate::types::{MemoryMib, VcpuCount};
use crate::vm::VmBuilder;
use crate::engine::VmEngine;

/// A vsock bridge entry in the config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeEntry {
    pub host_port: u16,
    pub guest_port: u32,
}

/// Top-level tateru configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TateruConfig {
    /// Number of vCPUs.
    #[serde(default = "default_cpus")]
    pub cpus: u8,

    /// Memory specification (e.g. `"8GiB"`).
    #[serde(default = "default_memory")]
    pub memory: String,

    /// Path to the disk image.
    pub disk: PathBuf,

    /// Disk format (default: qcow2).
    #[serde(default = "default_disk_format")]
    pub disk_format: DiskFormat,

    /// Whether the disk is read-only.
    #[serde(default)]
    pub disk_read_only: bool,

    /// Virtiofs mounts.
    #[serde(default)]
    pub virtiofs: Vec<VirtioFsMount>,

    /// Vsock bridges.
    #[serde(default)]
    pub vsock_bridges: Vec<BridgeEntry>,
}

fn default_cpus() -> u8 {
    6
}

fn default_memory() -> String {
    "8GiB".into()
}

fn default_disk_format() -> DiskFormat {
    DiskFormat::Qcow2
}

impl TateruConfig {
    /// Convert this config into a [`VmBuilder`].
    pub fn into_builder<E: VmEngine>(self, engine: E) -> Result<VmBuilder<E>, TateruError> {
        let vcpus = VcpuCount::new(self.cpus)?;
        let memory = MemoryMib::parse(&self.memory)?;

        let mut builder = VmBuilder::new(engine)
            .vcpus(vcpus)
            .memory_mib(memory)
            .disk(DiskConfig {
                path: self.disk,
                format: self.disk_format,
                read_only: self.disk_read_only,
            });

        for mount in self.virtiofs {
            builder = builder.virtiofs(mount);
        }

        for bridge in self.vsock_bridges {
            builder = builder.vsock_bridge(bridge.host_port, bridge.guest_port)?;
        }

        Ok(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        assert_eq!(default_cpus(), 6);
        assert_eq!(default_memory(), "8GiB");
        assert_eq!(default_disk_format(), DiskFormat::Qcow2);
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = TateruConfig {
            cpus: 4,
            memory: "4GiB".into(),
            disk: PathBuf::from("/var/lib/vm/guest.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![VirtioFsMount {
                host_path: "/shared".into(),
                mount_tag: "data".into(),
            }],
            vsock_bridges: vec![BridgeEntry {
                host_port: 31122,
                guest_port: 22,
            }],
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: TateruConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.cpus, 4);
        assert_eq!(cfg2.memory, "4GiB");
        assert_eq!(cfg2.virtiofs.len(), 1);
        assert_eq!(cfg2.vsock_bridges.len(), 1);
    }

    #[test]
    fn config_into_builder() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 6,
            memory: "8GiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![BridgeEntry {
                host_port: 31122,
                guest_port: 22,
            }],
        };

        let builder = cfg.into_builder(MockEngine::new()).unwrap();
        assert_eq!(builder.disks.len(), 1);
        assert_eq!(builder.bridges.len(), 1);
    }

    #[test]
    fn config_into_builder_invalid_cpus() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 0,
            memory: "8GiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![],
        };

        let err = cfg.into_builder(MockEngine::new()).unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn config_into_builder_invalid_memory() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 4,
            memory: "garbage".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![],
        };

        let err = cfg.into_builder(MockEngine::new()).unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn config_into_builder_invalid_bridge_port() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 4,
            memory: "4GiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![BridgeEntry {
                host_port: 31122,
                guest_port: 0, // invalid
            }],
        };

        let err = cfg.into_builder(MockEngine::new()).unwrap_err();
        assert!(err.to_string().contains("> 0"));
    }

    // -- Deserialization with defaults --
    // Catches bugs where serde defaults produce invalid values.

    #[test]
    fn config_deserialize_minimal() {
        let json = r#"{"disk": "/test.qcow2"}"#;
        let cfg: TateruConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.cpus, 6); // default
        assert_eq!(cfg.memory, "8GiB"); // default
        assert_eq!(cfg.disk, PathBuf::from("/test.qcow2"));
        assert_eq!(cfg.disk_format, DiskFormat::Qcow2); // default
        assert!(!cfg.disk_read_only); // default
        assert!(cfg.virtiofs.is_empty()); // default
        assert!(cfg.vsock_bridges.is_empty()); // default
    }

    #[test]
    fn config_deserialize_disk_read_only_true() {
        let json = r#"{"disk": "/test.raw", "disk_format": "raw", "disk_read_only": true}"#;
        let cfg: TateruConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.disk_read_only);
        assert_eq!(cfg.disk_format, DiskFormat::Raw);
    }

    // -- Multiple mounts and bridges --

    #[test]
    fn config_multiple_virtiofs_and_bridges() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 4,
            memory: "4GiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![
                VirtioFsMount {
                    host_path: "/shared1".into(),
                    mount_tag: "tag1".into(),
                },
                VirtioFsMount {
                    host_path: "/shared2".into(),
                    mount_tag: "tag2".into(),
                },
            ],
            vsock_bridges: vec![
                BridgeEntry { host_port: 31122, guest_port: 22 },
                BridgeEntry { host_port: 31180, guest_port: 80 },
            ],
        };

        let builder = cfg.into_builder(MockEngine::new()).unwrap();
        assert_eq!(builder.disks.len(), 1);
        assert_eq!(builder.virtiofs_mounts.len(), 2);
        assert_eq!(builder.bridges.len(), 2);
    }

    // -- Config with read-only disk --

    #[test]
    fn config_into_builder_read_only_disk() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 2,
            memory: "2GiB".into(),
            disk: PathBuf::from("/readonly.raw"),
            disk_format: DiskFormat::Raw,
            disk_read_only: true,
            virtiofs: vec![],
            vsock_bridges: vec![],
        };

        let builder = cfg.into_builder(MockEngine::new()).unwrap();
        assert_eq!(builder.disks.len(), 1);
        assert!(builder.disks[0].read_only);
        assert_eq!(builder.disks[0].format, DiskFormat::Raw);
    }

    // -- BridgeEntry serde --

    #[test]
    fn bridge_entry_serde_roundtrip() {
        let entry = BridgeEntry {
            host_port: 31122,
            guest_port: 22,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let entry2: BridgeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry2.host_port, 31122);
        assert_eq!(entry2.guest_port, 22);
    }

    #[test]
    fn bridge_entry_debug() {
        let entry = BridgeEntry {
            host_port: 31122,
            guest_port: 22,
        };
        let d = format!("{entry:?}");
        assert!(d.contains("31122"));
        assert!(d.contains("22"));
    }

    // -- TateruConfig Clone + Debug --

    #[test]
    fn config_clone() {
        let cfg = TateruConfig {
            cpus: 4,
            memory: "4GiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![],
        };
        let cfg2 = cfg.clone();
        assert_eq!(cfg2.cpus, 4);
        assert_eq!(cfg2.memory, "4GiB");
    }

    #[test]
    fn config_debug() {
        let cfg = TateruConfig {
            cpus: 4,
            memory: "4GiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![],
        };
        let d = format!("{cfg:?}");
        assert!(d.contains("TateruConfig"));
    }

    // -- Config with MiB memory specification --

    #[test]
    fn config_into_builder_mib_memory() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 2,
            memory: "2048MiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![],
        };

        // Should succeed without error (memory parsed correctly)
        cfg.into_builder(MockEngine::new()).unwrap();
    }

    // -- Config with max cpus --

    #[test]
    fn config_into_builder_max_cpus() {
        use crate::engine::mock::MockEngine;

        let cfg = TateruConfig {
            cpus: 255,
            memory: "1GiB".into(),
            disk: PathBuf::from("/test.qcow2"),
            disk_format: DiskFormat::Qcow2,
            disk_read_only: false,
            virtiofs: vec![],
            vsock_bridges: vec![],
        };

        // Should succeed — 255 is the max valid vCPU count
        cfg.into_builder(MockEngine::new()).unwrap();
    }

    // -- Deserialize with all fields explicitly set --

    #[test]
    fn config_deserialize_all_fields() {
        let json = r#"{
            "cpus": 8,
            "memory": "16GiB",
            "disk": "/vm/guest.qcow2",
            "disk_format": "qcow2",
            "disk_read_only": false,
            "virtiofs": [{"host_path": "/shared", "mount_tag": "data"}],
            "vsock_bridges": [{"host_port": 31122, "guest_port": 22}]
        }"#;
        let cfg: TateruConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.cpus, 8);
        assert_eq!(cfg.memory, "16GiB");
        assert_eq!(cfg.virtiofs.len(), 1);
        assert_eq!(cfg.vsock_bridges.len(), 1);
    }
}
