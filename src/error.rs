use std::path::PathBuf;

/// Errors produced by tateru VM operations.
#[derive(Debug, thiserror::Error)]
pub enum TateruError {
    /// libkrun FFI call returned a negative error code.
    #[error("libkrun {function} failed: error code {code}")]
    Ffi { function: &'static str, code: i32 },

    /// A required path was not valid UTF-8 (libkrun needs C strings).
    #[error("path is not valid UTF-8: {0}")]
    InvalidPath(PathBuf),

    /// VM builder validation failed.
    #[error("invalid VM config: {0}")]
    InvalidConfig(String),

    /// The VM is not running when an operation requires it.
    #[error("VM is not running")]
    NotRunning,

    /// The VM is already running when launch is attempted.
    #[error("VM is already running")]
    AlreadyRunning,

    /// The VM thread panicked.
    #[error("VM thread panicked")]
    VmThreadPanicked,

    /// vsock bridge I/O error.
    #[error("bridge I/O error: {0}")]
    BridgeIo(#[from] std::io::Error),

    /// Memory string could not be parsed.
    #[error("invalid memory specification: {0}")]
    InvalidMemory(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_ffi_error() {
        let e = TateruError::Ffi {
            function: "krun_create_ctx",
            code: -22,
        };
        assert_eq!(
            e.to_string(),
            "libkrun krun_create_ctx failed: error code -22"
        );
    }

    #[test]
    fn display_invalid_path() {
        let e = TateruError::InvalidPath(PathBuf::from("/bad/path"));
        assert_eq!(e.to_string(), "path is not valid UTF-8: /bad/path");
    }

    #[test]
    fn display_invalid_config() {
        let e = TateruError::InvalidConfig("no disk configured".into());
        assert_eq!(e.to_string(), "invalid VM config: no disk configured");
    }

    #[test]
    fn display_not_running() {
        let e = TateruError::NotRunning;
        assert_eq!(e.to_string(), "VM is not running");
    }

    #[test]
    fn display_already_running() {
        let e = TateruError::AlreadyRunning;
        assert_eq!(e.to_string(), "VM is already running");
    }

    #[test]
    fn display_vm_thread_panicked() {
        let e = TateruError::VmThreadPanicked;
        assert_eq!(e.to_string(), "VM thread panicked");
    }

    #[test]
    fn display_bridge_io() {
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let e = TateruError::BridgeIo(io);
        assert_eq!(e.to_string(), "bridge I/O error: refused");
    }

    #[test]
    fn display_invalid_memory() {
        let e = TateruError::InvalidMemory("lots".into());
        assert_eq!(e.to_string(), "invalid memory specification: lots");
    }

    #[test]
    fn from_io_error() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let e: TateruError = io.into();
        assert!(matches!(e, TateruError::BridgeIo(_)));
    }

    #[test]
    fn error_is_debug() {
        let e = TateruError::NotRunning;
        let debug = format!("{e:?}");
        assert!(debug.contains("NotRunning"));
    }

    // Verify std::error::Error source() chain propagation for BridgeIo.
    // Catches bugs where the #[from] attribute is missing or source() returns None.

    #[test]
    fn bridge_io_has_source() {
        use std::error::Error;
        let io = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
        let e = TateruError::BridgeIo(io);
        let source = e.source().expect("BridgeIo should have a source");
        assert!(source.to_string().contains("broken"));
    }

    #[test]
    fn ffi_error_no_source() {
        use std::error::Error;
        let e = TateruError::Ffi {
            function: "krun_test",
            code: -1,
        };
        assert!(e.source().is_none());
    }

    #[test]
    fn invalid_config_no_source() {
        use std::error::Error;
        let e = TateruError::InvalidConfig("test".into());
        assert!(e.source().is_none());
    }

    #[test]
    fn invalid_memory_no_source() {
        use std::error::Error;
        let e = TateruError::InvalidMemory("test".into());
        assert!(e.source().is_none());
    }

    #[test]
    fn invalid_path_no_source() {
        use std::error::Error;
        let e = TateruError::InvalidPath(PathBuf::from("/test"));
        assert!(e.source().is_none());
    }

    // Verify TateruError is Send (required for async error propagation).
    #[test]
    fn error_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<TateruError>();
    }

    // Verify all variant Display messages are non-empty.
    #[test]
    fn all_variants_have_nonempty_display() {
        let variants: Vec<TateruError> = vec![
            TateruError::Ffi { function: "f", code: -1 },
            TateruError::InvalidPath(PathBuf::from("/p")),
            TateruError::InvalidConfig("c".into()),
            TateruError::NotRunning,
            TateruError::AlreadyRunning,
            TateruError::VmThreadPanicked,
            TateruError::BridgeIo(std::io::Error::new(std::io::ErrorKind::Other, "e")),
            TateruError::InvalidMemory("m".into()),
        ];
        for v in &variants {
            assert!(!v.to_string().is_empty(), "empty display for {v:?}");
        }
    }

    // Verify all variant Debug strings are non-empty.
    #[test]
    fn all_variants_have_nonempty_debug() {
        let variants: Vec<TateruError> = vec![
            TateruError::Ffi { function: "f", code: -1 },
            TateruError::InvalidPath(PathBuf::from("/p")),
            TateruError::InvalidConfig("c".into()),
            TateruError::NotRunning,
            TateruError::AlreadyRunning,
            TateruError::VmThreadPanicked,
            TateruError::BridgeIo(std::io::Error::new(std::io::ErrorKind::Other, "e")),
            TateruError::InvalidMemory("m".into()),
        ];
        for v in &variants {
            assert!(!format!("{v:?}").is_empty());
        }
    }
}
