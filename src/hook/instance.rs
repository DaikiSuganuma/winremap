//! The named mutex that keeps a second WinRemap from installing a second
//! hook (project brief §9-3, ADR 0009).

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

/// Owns the named mutex that prevents a second winremap instance — two
/// processes would install two low-level hooks with undefined interleaving
/// (brief §9-3). Lives in hook.rs because it protects hook integrity, and
/// unsafe is confined to this module (AGENTS.md invariant 3, ADR 0009).
pub struct SingleInstance(HANDLE);

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: the handle was created by acquire_single_instance and is
        // closed exactly once here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// `Ok(None)` means another instance already holds the mutex. The `Local\`
/// namespace scopes it per login session on purpose: hooks are per-session,
/// so two different sessions may each run their own winremap.
pub fn acquire_single_instance() -> windows::core::Result<Option<SingleInstance>> {
    // SAFETY: the name is a static wide string; the returned handle is owned
    // by SingleInstance and closed on drop.
    let handle = unsafe { CreateMutexW(None, false, w!("Local\\winremap-single-instance")) }?;
    // CreateMutexW succeeds even when the mutex exists; only the last-error
    // state distinguishes "created" from "opened someone else's".
    // SAFETY: reads calling thread's last-error slot, set by the call above.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        drop(SingleInstance(handle));
        Ok(None)
    } else {
        Ok(Some(SingleInstance(handle)))
    }
}
