//! Virtualization.framework engine — Apple's high-level VMM, the
//! second tateru engine alongside [`crate::engine::LibkrunEngine`].
//!
//! Why two engines.
//!
//! - libkrun talks Hypervisor.framework directly. Lighter, faster
//!   boot, full vsock + virtiofs control.
//! - VZ wraps Hypervisor.framework with conveniences libkrun doesn't
//!   re-implement. The big one is **Apple Rosetta-for-Linux**: the
//!   guest-side rosetta binary refuses to translate unless the host
//!   VMM enables `cpu.rosettaModeEnabled` on a `VZVirtualMachine`.
//!   That flag exists *only* on VZ — there is no way to set it from
//!   raw Hypervisor.framework calls.
//!
//! Result: aarch64-linux Rust compiles run on libkrun (faster, native
//! arm64 hardware execution); x86_64-linux Rust compiles run on VZ
//! (Rosetta-translated, ~5–10× faster than qemu-user TCG).
//!
//! Same `Engine` trait, two impls, same vsock-bridge wiring.
//!
//! Status: M0 — scaffold + types. Subsequent commits flesh out:
//!   M1: minimal VZ launch (CPU + RAM + disk + console)
//!   M2: vsock device + listener (host-initiated inbound)
//!   M3: virtiofs share for SSH keys
//!   M4: VZLinuxRosettaDirectoryShare for the rosetta runtime
//!   M5: feature parity with LibkrunEngine + libkrun-builder uses VZ
//!       when `:systems` declares `x86_64-linux`.
#![cfg(all(target_os = "macos", feature = "vz"))]

mod engine;
mod state;

pub use engine::VzEngine;
