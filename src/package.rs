//! Whether this process runs from an MSIX package, and where the files it
//! writes under `%APPDATA%` really live when it does (ADR 0061).
//!
//! A packaged desktop app has its `%APPDATA%` writes redirected to a private
//! per-package folder. The redirection is invisible to the app itself — it
//! opens `%APPDATA%\winremap\config.toml` and gets the right file — but not
//! to anyone else: Explorer and the text editor WinRemap hands the path to
//! are separate, unpackaged processes, and for them that path does not
//! exist. Resolving once at startup keeps every later use — the address bar,
//! the file watch, "open in text editor" — on one path that is true for
//! everybody.
//!
//! Unpackaged, every function here is a no-op returning its input.

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;
use windows::core::PWSTR;

/// Where a packaged app's redirected roaming data lands, relative to
/// `%LOCALAPPDATA%`. Documented under "AppData operations" in
/// <https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes>.
const PACKAGES: &str = "Packages";
const REDIRECT_TAIL: [&str; 2] = ["LocalCache", "Roaming"];

/// Resolves a config path to one that every process on the machine agrees
/// about. Returns `path` unchanged when unpackaged, or when it is not under
/// `%APPDATA%` (a `--config` elsewhere is never redirected).
///
/// Also the note of which config file to open next time (ADR 0077): it lives
/// in that same folder, and a path written under one spelling of `%APPDATA%`
/// and read under the other names a file that is not there.
pub fn resolve_config_path(path: PathBuf) -> PathBuf {
    let Some(family) = family_name() else {
        return path;
    };
    let (Some(appdata), Some(local)) = (
        std::env::var_os("APPDATA"),
        std::env::var_os("LOCALAPPDATA"),
    ) else {
        return path;
    };
    let Some(redirected) = redirect(&path, Path::new(&appdata), Path::new(&local), family) else {
        return path;
    };

    // Mirror the OS's own rule: it opens the package-private copy first and
    // falls back to the real AppData file only when the private one is
    // absent. A config left behind by an installed WinRemap therefore keeps
    // being used, rather than being shadowed by a fresh default.
    if redirected.exists() || !path.exists() {
        redirected
    } else {
        path
    }
}

/// The redirected location of `path`, or `None` when it is not under
/// `appdata`. Pure, so the mapping can be tested off a real package.
fn redirect(path: &Path, appdata: &Path, local: &Path, family: &OsStr) -> Option<PathBuf> {
    let relative = path.strip_prefix(appdata).ok()?;
    let mut out = local.join(PACKAGES).join(family);
    out.extend(REDIRECT_TAIL);
    Some(out.join(relative))
}

/// This process's package family name, or `None` when it is not packaged.
/// Asked once — a process cannot change packages while it runs.
fn family_name() -> Option<&'static OsString> {
    static FAMILY: OnceLock<Option<OsString>> = OnceLock::new();
    FAMILY.get_or_init(query_family_name).as_ref()
}

fn query_family_name() -> Option<OsString> {
    let mut length = 0u32;
    // SAFETY: the documented size query — a null buffer with a zero length,
    // which makes the call write only through `length`.
    let status = unsafe { GetCurrentPackageFamilyName(&mut length, None) };
    // Anything else (APPMODEL_ERROR_NO_PACKAGE above all) means unpackaged,
    // which is the ordinary case and not worth reporting.
    if status != ERROR_INSUFFICIENT_BUFFER {
        return None;
    }

    let mut buffer = vec![0u16; length as usize];
    // SAFETY: `length` is the size the call above asked for, and `buffer`
    // holds that many u16 and outlives the call.
    let status =
        unsafe { GetCurrentPackageFamilyName(&mut length, Some(PWSTR(buffer.as_mut_ptr()))) };
    if status != ERROR_SUCCESS {
        return None;
    }

    // The returned length counts the terminating null.
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(OsString::from_wide(&buffer[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPDATA: &str = r"C:\Users\u\AppData\Roaming";
    const LOCAL: &str = r"C:\Users\u\AppData\Local";
    const FAMILY: &str = "SUGANUMADaiki.WinRemap_pktmgf1zdhxe0";

    fn map(path: &str) -> Option<PathBuf> {
        redirect(
            Path::new(path),
            Path::new(APPDATA),
            Path::new(LOCAL),
            OsStr::new(FAMILY),
        )
    }

    #[test]
    fn a_config_under_appdata_maps_into_the_package_store() {
        assert_eq!(
            map(&format!(r"{APPDATA}\winremap\config.toml")),
            Some(PathBuf::from(format!(
                r"{LOCAL}\Packages\{FAMILY}\LocalCache\Roaming\winremap\config.toml"
            )))
        );
    }

    #[test]
    fn a_path_outside_appdata_is_not_redirected() {
        // `--config D:\work\keys.toml` is the user's own file; the package
        // store has nothing to do with it.
        assert_eq!(map(r"D:\work\keys.toml"), None);
    }

    #[test]
    fn appdata_itself_maps_to_the_roaming_root() {
        assert_eq!(
            map(APPDATA),
            Some(PathBuf::from(format!(
                r"{LOCAL}\Packages\{FAMILY}\LocalCache\Roaming"
            )))
        );
    }
}
