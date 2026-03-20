//! Shutdown handle wrapping the libkrun eventfd.

use std::os::fd::FromRawFd;
use std::os::fd::OwnedFd;

use crate::error::TateruError;

/// Handle to trigger VM shutdown via the libkrun eventfd.
///
/// Writing to the eventfd signals the guest to shut down orderly.
pub struct ShutdownHandle {
    fd: OwnedFd,
}

impl ShutdownHandle {
    /// Create from a raw eventfd file descriptor returned by `krun_get_shutdown_eventfd`.
    ///
    /// # Safety
    ///
    /// The caller must ensure `raw_fd` is a valid, open file descriptor
    /// that was returned by `krun_get_shutdown_eventfd`.
    pub(crate) unsafe fn from_raw_fd(raw_fd: i32) -> Self {
        Self {
            fd: unsafe { OwnedFd::from_raw_fd(raw_fd) },
        }
    }

    /// Signal the VM to shut down by writing to the eventfd.
    pub fn trigger(&self) -> Result<(), TateruError> {
        use std::os::fd::AsRawFd;

        let val: u64 = 1;
        let ret = unsafe {
            libc::write(
                self.fd.as_raw_fd(),
                std::ptr::from_ref(&val).cast(),
                std::mem::size_of::<u64>(),
            )
        };
        if ret < 0 {
            Err(TateruError::BridgeIo(std::io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }
}

impl std::fmt::Debug for ShutdownHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::os::fd::AsRawFd;
        f.debug_struct("ShutdownHandle")
            .field("fd", &self.fd.as_raw_fd())
            .finish()
    }
}
