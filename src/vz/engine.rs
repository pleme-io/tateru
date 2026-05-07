//! `VzEngine` — `VmEngine` impl backed by Apple Virtualization.framework.
//!
//! M0 status: scaffold only. Every trait method either records into
//! `VzCtxState` or returns `TateruError::Unsupported` for the parts
//! that need actual `VZ*` config types (deferred to M1+). The point
//! of this commit is to land the typed surface so consumers can
//! choose engine via Cargo feature without breaking.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::devices::{ConsoleConfig, DiskConfig, VirtioFsMount, VsockPort};
use crate::engine::VmEngine;
use crate::error::TateruError;
use crate::types::{CtxId, LogLevel, MemoryMib, VcpuCount};
use crate::vz::state::VzCtxState;

/// VZ-backed VM engine. Holds per-context state until `start_enter`
/// materializes the actual `VZVirtualMachine`.
#[derive(Debug, Default)]
pub struct VzEngine {
    contexts: Mutex<HashMap<CtxId, VzCtxState>>,
    next_id: AtomicU32,
}

impl VzEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with_ctx<R>(
        &self,
        ctx: CtxId,
        f: impl FnOnce(&mut VzCtxState) -> Result<R, TateruError>,
    ) -> Result<R, TateruError> {
        let mut map = self
            .contexts
            .lock()
            .map_err(|_| TateruError::InvalidConfig("vz state lock poisoned".into()))?;
        let state = map
            .get_mut(&ctx)
            .ok_or_else(|| TateruError::InvalidConfig(format!("vz: unknown ctx {}", ctx.0)))?;
        f(state)
    }
}

impl VmEngine for VzEngine {
    fn create_ctx(&self) -> Result<CtxId, TateruError> {
        let id = CtxId(self.next_id.fetch_add(1, Ordering::SeqCst));
        let mut map = self
            .contexts
            .lock()
            .map_err(|_| TateruError::InvalidConfig("vz state lock poisoned".into()))?;
        map.insert(id, VzCtxState::default());
        Ok(id)
    }

    fn set_vm_config(
        &self,
        ctx: CtxId,
        vcpus: VcpuCount,
        memory: MemoryMib,
    ) -> Result<(), TateruError> {
        self.with_ctx(ctx, |state| {
            state.vcpus = Some(vcpus);
            state.memory = Some(memory);
            Ok(())
        })
    }

    fn add_disk(
        &self,
        ctx: CtxId,
        disk: &DiskConfig,
        _index: usize,
    ) -> Result<(), TateruError> {
        self.with_ctx(ctx, |state| {
            state.disks.push(disk.clone());
            Ok(())
        })
    }

    fn add_virtiofs(
        &self,
        ctx: CtxId,
        mount: &VirtioFsMount,
    ) -> Result<(), TateruError> {
        self.with_ctx(ctx, |state| {
            state.virtiofs.push(mount.clone());
            Ok(())
        })
    }

    fn add_vsock_port(
        &self,
        ctx: CtxId,
        port: &VsockPort,
    ) -> Result<(), TateruError> {
        self.with_ctx(ctx, |state| {
            state.vsock_ports.push(port.clone());
            Ok(())
        })
    }

    fn set_console_output(
        &self,
        ctx: CtxId,
        console: &ConsoleConfig,
    ) -> Result<(), TateruError> {
        self.with_ctx(ctx, |state| {
            state.console = Some(console.clone());
            Ok(())
        })
    }

    fn get_shutdown_eventfd(&self, _ctx: CtxId) -> Result<i32, TateruError> {
        // VZ doesn't expose an eventfd-shaped shutdown channel.
        // The Rust-side caller observes shutdown via
        // `VZVirtualMachineDelegate.virtualMachineDidStop` callbacks,
        // wired in M2 (vsock + run loop integration).
        Err(TateruError::InvalidConfig(
            "vz: shutdown eventfd not available; use VM observer (M2)".into(),
        ))
    }

    fn start_enter(&self, _ctx: CtxId) -> Result<(), TateruError> {
        // M0 stub. M1 builds the VZVirtualMachineConfiguration from
        // VzCtxState, calls `.validate()`, then `.start()`, and runs
        // the dispatch queue forever (matching libkrun's
        // `krun_start_enter` semantics).
        Err(TateruError::InvalidConfig(
            "vz: start_enter not implemented yet (M1 deliverable)".into(),
        ))
    }

    fn set_log_level(&self, _level: LogLevel) -> Result<(), TateruError> {
        // VZ logs through os_log; no per-engine knob. Effectively
        // controlled via the `OS_ACTIVITY_DT_MODE` env var.
        Ok(())
    }

    fn check_nested_virt(&self) -> Result<bool, TateruError> {
        // M2: query
        // `VZGenericPlatformConfiguration.isNestedVirtualizationSupported`.
        Ok(false)
    }

    fn set_nested_virt(&self, _ctx: CtxId, _enabled: bool) -> Result<(), TateruError> {
        // M2: route to platform.isNestedVirtualizationEnabled.
        Ok(())
    }

    fn set_smbios_oem_strings(
        &self,
        _ctx: CtxId,
        _strings: &[&str],
    ) -> Result<(), TateruError> {
        // VZ doesn't surface SMBIOS OEM strings the way libkrun does;
        // closest analogue is `VZGenericMachineIdentifier`. M2.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_ctx_assigns_distinct_ids() {
        let e = VzEngine::new();
        let a = e.create_ctx().unwrap();
        let b = e.create_ctx().unwrap();
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn set_vm_config_persists_in_state() {
        let e = VzEngine::new();
        let ctx = e.create_ctx().unwrap();
        e.set_vm_config(ctx, VcpuCount::new(4).unwrap(), MemoryMib::new(2048).unwrap())
            .unwrap();
        let map = e.contexts.lock().unwrap();
        let state = map.get(&ctx).unwrap();
        assert_eq!(state.vcpus.unwrap().raw(), 4);
        assert_eq!(state.memory.unwrap().raw(), 2048);
    }

    #[test]
    fn unknown_ctx_returns_error() {
        let e = VzEngine::new();
        let bogus = CtxId(99_999);
        let err = e
            .set_vm_config(bogus, VcpuCount::new(1).unwrap(), MemoryMib::new(512).unwrap())
            .unwrap_err();
        assert!(matches!(err, TateruError::InvalidConfig(_)));
    }

    #[test]
    fn rosetta_share_is_recorded_for_later_materialization() {
        use std::path::PathBuf;
        let e = VzEngine::new();
        let ctx = e.create_ctx().unwrap();
        let mount = VirtioFsMount {
            mount_tag: "rosetta".into(),
            host_path: PathBuf::from("/Library/Apple/usr/libexec/oah"),
        };
        e.add_virtiofs(ctx, &mount).unwrap();
        let map = e.contexts.lock().unwrap();
        let state = map.get(&ctx).unwrap();
        assert_eq!(state.virtiofs.len(), 1);
        assert_eq!(state.virtiofs[0].mount_tag, "rosetta");
    }
}
