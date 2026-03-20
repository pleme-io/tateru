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
}
