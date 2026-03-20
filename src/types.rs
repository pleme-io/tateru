//! Strong newtypes wrapping raw FFI values.
//!
//! These types prevent accidental misuse of raw integers at the FFI boundary.
//! None of them can be constructed outside this crate — consumers receive them
//! from [`VmEngine`](crate::engine::VmEngine) methods.

use serde::{Deserialize, Serialize};

use crate::error::TateruError;

/// VM context identifier returned by `krun_create_ctx`.
///
/// Opaque handle — cannot be constructed outside the crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CtxId(pub(crate) u32);

impl CtxId {
    /// Raw numeric value (for logging/debugging only).
    #[inline]
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for CtxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ctx:{}", self.0)
    }
}

/// Number of virtual CPUs for the VM.
///
/// Validated on construction: must be 1–255 (u8 range, capped by libkrun).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VcpuCount(u8);

impl VcpuCount {
    /// Create a validated vCPU count.
    ///
    /// # Errors
    ///
    /// Returns `InvalidConfig` if `count` is 0.
    pub fn new(count: u8) -> Result<Self, TateruError> {
        if count == 0 {
            return Err(TateruError::InvalidConfig(
                "vCPU count must be at least 1".into(),
            ));
        }
        Ok(Self(count))
    }

    /// Raw u8 value for FFI.
    #[inline]
    #[must_use]
    pub fn raw(self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for VcpuCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} vCPU(s)", self.0)
    }
}

/// VM memory in mebibytes.
///
/// Validated on construction: must be > 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMib(u32);

impl MemoryMib {
    /// Create from a raw MiB value.
    ///
    /// # Errors
    ///
    /// Returns `InvalidConfig` if `mib` is 0.
    pub fn new(mib: u32) -> Result<Self, TateruError> {
        if mib == 0 {
            return Err(TateruError::InvalidConfig(
                "memory must be at least 1 MiB".into(),
            ));
        }
        Ok(Self(mib))
    }

    /// Parse a human-readable memory string.
    ///
    /// Accepts: `"8GiB"`, `"8192MiB"`, `"8192"` (plain MiB).
    pub fn parse(s: &str) -> Result<Self, TateruError> {
        let lower = s.trim().to_lowercase();
        let mib = if let Some(g) = lower.strip_suffix("gib") {
            g.trim()
                .parse::<u32>()
                .map_err(|_| TateruError::InvalidMemory(s.into()))?
                .checked_mul(1024)
                .ok_or_else(|| TateruError::InvalidMemory(s.into()))?
        } else if let Some(m) = lower.strip_suffix("mib") {
            m.trim()
                .parse::<u32>()
                .map_err(|_| TateruError::InvalidMemory(s.into()))?
        } else {
            lower
                .parse::<u32>()
                .map_err(|_| TateruError::InvalidMemory(s.into()))?
        };
        Self::new(mib)
    }

    /// Raw u32 value for FFI.
    #[inline]
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for MemoryMib {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 >= 1024 && self.0 % 1024 == 0 {
            write!(f, "{}GiB", self.0 / 1024)
        } else {
            write!(f, "{}MiB", self.0)
        }
    }
}

/// libkrun log verbosity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum LogLevel {
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl LogLevel {
    /// Raw u32 value for FFI.
    #[inline]
    #[must_use]
    pub fn raw(self) -> u32 {
        self as u32
    }
}

/// vsock port number on the guest side.
///
/// Validated on construction: must be > 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GuestPort(u32);

impl GuestPort {
    /// Create a validated guest port.
    ///
    /// # Errors
    ///
    /// Returns `InvalidConfig` if `port` is 0.
    pub fn new(port: u32) -> Result<Self, TateruError> {
        if port == 0 {
            return Err(TateruError::InvalidConfig(
                "guest port must be > 0".into(),
            ));
        }
        Ok(Self(port))
    }

    /// Raw u32 value for FFI.
    #[inline]
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for GuestPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vsock:{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- CtxId --

    #[test]
    fn ctx_id_display() {
        let id = CtxId(42);
        assert_eq!(id.to_string(), "ctx:42");
    }

    #[test]
    fn ctx_id_raw() {
        let id = CtxId(7);
        assert_eq!(id.raw(), 7);
    }

    #[test]
    fn ctx_id_equality() {
        assert_eq!(CtxId(1), CtxId(1));
        assert_ne!(CtxId(1), CtxId(2));
    }

    #[test]
    fn ctx_id_copy() {
        let a = CtxId(5);
        let b = a;
        assert_eq!(a, b);
    }

    // -- VcpuCount --

    #[test]
    fn vcpu_count_valid() {
        let v = VcpuCount::new(6).unwrap();
        assert_eq!(v.raw(), 6);
    }

    #[test]
    fn vcpu_count_one() {
        let v = VcpuCount::new(1).unwrap();
        assert_eq!(v.raw(), 1);
    }

    #[test]
    fn vcpu_count_max() {
        let v = VcpuCount::new(255).unwrap();
        assert_eq!(v.raw(), 255);
    }

    #[test]
    fn vcpu_count_zero_rejected() {
        let err = VcpuCount::new(0).unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn vcpu_count_display() {
        let v = VcpuCount::new(4).unwrap();
        assert_eq!(v.to_string(), "4 vCPU(s)");
    }

    #[test]
    fn vcpu_count_serde_roundtrip() {
        let v = VcpuCount::new(8).unwrap();
        let json = serde_json::to_string(&v).unwrap();
        let v2: VcpuCount = serde_json::from_str(&json).unwrap();
        assert_eq!(v, v2);
    }

    // -- MemoryMib --

    #[test]
    fn memory_mib_raw() {
        let m = MemoryMib::new(4096).unwrap();
        assert_eq!(m.raw(), 4096);
    }

    #[test]
    fn memory_mib_zero_rejected() {
        let err = MemoryMib::new(0).unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn memory_mib_parse_plain() {
        let m = MemoryMib::parse("4096").unwrap();
        assert_eq!(m.raw(), 4096);
    }

    #[test]
    fn memory_mib_parse_gib() {
        let m = MemoryMib::parse("8GiB").unwrap();
        assert_eq!(m.raw(), 8192);
    }

    #[test]
    fn memory_mib_parse_gib_lowercase() {
        let m = MemoryMib::parse("4gib").unwrap();
        assert_eq!(m.raw(), 4096);
    }

    #[test]
    fn memory_mib_parse_mib() {
        let m = MemoryMib::parse("2048MiB").unwrap();
        assert_eq!(m.raw(), 2048);
    }

    #[test]
    fn memory_mib_parse_mib_lowercase() {
        let m = MemoryMib::parse("1024mib").unwrap();
        assert_eq!(m.raw(), 1024);
    }

    #[test]
    fn memory_mib_parse_with_spaces() {
        let m = MemoryMib::parse("  16 GiB  ").unwrap();
        assert_eq!(m.raw(), 16384);
    }

    #[test]
    fn memory_mib_parse_invalid() {
        let err = MemoryMib::parse("lots").unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn memory_mib_display_gib() {
        let m = MemoryMib::new(8192).unwrap();
        assert_eq!(m.to_string(), "8GiB");
    }

    #[test]
    fn memory_mib_display_mib() {
        let m = MemoryMib::new(1500).unwrap();
        assert_eq!(m.to_string(), "1500MiB");
    }

    #[test]
    fn memory_mib_serde_roundtrip() {
        let m = MemoryMib::new(8192).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let m2: MemoryMib = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }

    // -- LogLevel --

    #[test]
    fn log_level_raw_values() {
        assert_eq!(LogLevel::Off.raw(), 0);
        assert_eq!(LogLevel::Error.raw(), 1);
        assert_eq!(LogLevel::Warn.raw(), 2);
        assert_eq!(LogLevel::Info.raw(), 3);
        assert_eq!(LogLevel::Debug.raw(), 4);
        assert_eq!(LogLevel::Trace.raw(), 5);
    }

    // -- GuestPort --

    #[test]
    fn guest_port_valid() {
        let p = GuestPort::new(22).unwrap();
        assert_eq!(p.raw(), 22);
    }

    #[test]
    fn guest_port_zero_rejected() {
        let err = GuestPort::new(0).unwrap_err();
        assert!(err.to_string().contains("> 0"));
    }

    #[test]
    fn guest_port_display() {
        let p = GuestPort::new(8080).unwrap();
        assert_eq!(p.to_string(), "vsock:8080");
    }

    #[test]
    fn guest_port_serde_roundtrip() {
        let p = GuestPort::new(443).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        let p2: GuestPort = serde_json::from_str(&json).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn guest_port_equality() {
        assert_eq!(GuestPort::new(22).unwrap(), GuestPort::new(22).unwrap());
        assert_ne!(GuestPort::new(22).unwrap(), GuestPort::new(80).unwrap());
    }

    // -- MemoryMib overflow edge cases --

    #[test]
    fn memory_mib_parse_gib_overflow() {
        // 5_000_000 GiB * 1024 = 5_120_000_000 which overflows u32
        let err = MemoryMib::parse("5000000GiB").unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn memory_mib_parse_gib_just_below_overflow() {
        // 4_194_303 GiB * 1024 = 4_294_967_296 which overflows u32 (max is 4_294_967_295)
        let err = MemoryMib::parse("4194304GiB").unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn memory_mib_parse_gib_max_valid() {
        // u32::MAX / 1024 = 4_194_303 GiB
        let m = MemoryMib::parse("4194303GiB").unwrap();
        assert_eq!(m.raw(), 4_194_303 * 1024);
    }

    #[test]
    fn memory_mib_parse_u32_max() {
        let m = MemoryMib::new(u32::MAX).unwrap();
        assert_eq!(m.raw(), u32::MAX);
    }

    #[test]
    fn memory_mib_parse_empty_string() {
        let err = MemoryMib::parse("").unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn memory_mib_parse_only_suffix() {
        let err = MemoryMib::parse("GiB").unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn memory_mib_parse_negative() {
        let err = MemoryMib::parse("-1").unwrap_err();
        assert!(matches!(err, TateruError::InvalidMemory(_)));
    }

    #[test]
    fn memory_mib_parse_zero_gib() {
        // "0GiB" → 0 MiB → rejected by MemoryMib::new
        let err = MemoryMib::parse("0GiB").unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn memory_mib_parse_zero_mib() {
        let err = MemoryMib::parse("0MiB").unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn memory_mib_display_1gib() {
        let m = MemoryMib::new(1024).unwrap();
        assert_eq!(m.to_string(), "1GiB");
    }

    #[test]
    fn memory_mib_display_1mib() {
        let m = MemoryMib::new(1).unwrap();
        assert_eq!(m.to_string(), "1MiB");
    }

    // -- VcpuCount copy semantics --

    #[test]
    fn vcpu_count_copy_semantics() {
        let a = VcpuCount::new(4).unwrap();
        let b = a; // Copy
        let c = a; // Still valid after copy
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a.raw(), 4);
        assert_eq!(b.raw(), 4);
        assert_eq!(c.raw(), 4);
    }

    // -- CtxId hash --

    #[test]
    fn ctx_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(CtxId(1));
        set.insert(CtxId(2));
        set.insert(CtxId(1)); // duplicate
        assert_eq!(set.len(), 2);
    }

    // -- GuestPort max value --

    #[test]
    fn guest_port_max_value() {
        let p = GuestPort::new(u32::MAX).unwrap();
        assert_eq!(p.raw(), u32::MAX);
    }

    // -- GuestPort hash --

    #[test]
    fn guest_port_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(GuestPort::new(22).unwrap());
        set.insert(GuestPort::new(80).unwrap());
        set.insert(GuestPort::new(22).unwrap()); // duplicate
        assert_eq!(set.len(), 2);
    }

    // -- LogLevel copy --

    #[test]
    fn log_level_copy() {
        let a = LogLevel::Debug;
        let b = a;
        assert_eq!(a, b);
    }

    // -- LogLevel equality --

    #[test]
    fn log_level_equality() {
        assert_eq!(LogLevel::Off, LogLevel::Off);
        assert_ne!(LogLevel::Off, LogLevel::Error);
    }
}
