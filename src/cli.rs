//! The command line: the six flags `winremap` takes, parsed by hand.
//!
//! No argument-parsing crate: the surface is small and fixed, and every
//! dependency is a cost (guidelines §8). The parse has no side effects —
//! it does not touch the filesystem — so it is testable here, and creating
//! the config file stays the startup sequence's job (`main::ensure_config`).
//! The two things it does do besides returning are `--help` and `--version`,
//! which print and exit.

use std::path::PathBuf;

use anyhow::{Context, bail};

use crate::i18n;
use crate::notify;

pub struct CliArgs {
    pub config_path: PathBuf,
    /// `false` when `--config` named the path. Decides whether a missing file
    /// is created or reported (see `main::ensure_config`).
    pub config_is_default: bool,
    pub debug: bool,
    /// `None` when the flag was absent, so the config file's value applies.
    pub macro_delay_ms: Option<u32>,
    /// Test-only remapping of other software's injected events (ADR 0053).
    #[cfg(feature = "test-inject")]
    pub accept_injected: bool,
}

/// Pre-scan for `--lang` so i18n can initialize before any other output.
pub fn extract_lang(args: &[String]) -> anyhow::Result<Option<i18n::Lang>> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--lang" {
            return match iter.next().map(String::as_str) {
                Some("en") => Ok(Some(i18n::Lang::En)),
                Some("ja") => Ok(Some(i18n::Lang::Ja)),
                // English on purpose: i18n is not initialized yet.
                other => bail!(
                    "invalid --lang value `{}` (expected `en` or `ja`)",
                    other.unwrap_or("")
                ),
            };
        }
    }
    Ok(None)
}

pub fn parse_args(args: &[String]) -> anyhow::Result<CliArgs> {
    let mut config: Option<PathBuf> = None;
    let mut debug = false;
    let mut macro_delay_ms: Option<u32> = None;
    #[cfg(feature = "test-inject")]
    let mut accept_injected = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                let value = iter.next().context("--config requires a path")?;
                config = Some(PathBuf::from(value));
            }
            // Already consumed by extract_lang; skip its value here.
            "--lang" => {
                iter.next();
            }
            "--debug" => debug = true,
            // Absent from ordinary builds, where it falls to the unknown
            // argument arm below like any other typo (ADR 0053).
            #[cfg(feature = "test-inject")]
            "--accept-injected" => accept_injected = true,
            "--macro-delay" => {
                let value = iter.next().context("--macro-delay requires milliseconds")?;
                let max = winremap::keymap::MAX_MACRO_DELAY_MS;
                macro_delay_ms = Some(value.parse().ok().filter(|&ms| ms <= max).with_context(
                    || format!("invalid --macro-delay `{value}` (expected 0-{max})"),
                )?);
            }
            // Both go through notify so a shortcut carrying the flag still
            // shows something instead of exiting silently.
            "--version" | "-V" => {
                notify::info(&format!("winremap {}", env!("CARGO_PKG_VERSION")));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                notify::info(&i18n::help_text());
                std::process::exit(0);
            }
            other => bail!("{}", i18n::unknown_argument(other)),
        }
    }
    let config_is_default = config.is_none();
    let config_path = match config {
        Some(path) => path,
        None => default_config_path()?,
    };
    Ok(CliArgs {
        config_path,
        config_is_default,
        debug,
        macro_delay_ms,
        #[cfg(feature = "test-inject")]
        accept_injected,
    })
}

/// Where a config lives when `--config` was not given. Existence is not
/// checked here: `parse_args` stays free of filesystem side effects, and
/// creating the file is the startup sequence's job (`main::ensure_config`).
pub fn default_config_path() -> anyhow::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA")
        .context("APPDATA is not set; pass --config <path> explicitly")?;
    Ok(PathBuf::from(appdata).join("winremap").join("config.toml"))
}

/// Where the last chosen config file is remembered (ADR 0077): beside the
/// default config, whatever file this run ends up opening. `None` when
/// `%APPDATA%` is not set — the same condition that leaves
/// [`default_config_path`] with nothing to return, and one that makes the
/// feature quietly absent rather than fatal.
///
/// Deliberately not beside the *active* config: a `--config D:\work\keys.toml`
/// would otherwise leave a memory file in the user's own folder, and the next
/// plain start — which looks in `%APPDATA%` — would never read it.
pub fn memory_path() -> Option<PathBuf> {
    Some(
        default_config_path()
            .ok()?
            .with_file_name(winremap::config::last_used::FILE_NAME),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--config` keeps the cases off %APPDATA%, which may hold a real config.
    fn parse(extra: &[&str]) -> anyhow::Result<CliArgs> {
        let mut args = vec!["--config".to_owned(), "x.toml".to_owned()];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        parse_args(&args)
    }

    /// Decision 2 of ADR 0077: the memory sits beside the **default** config,
    /// whatever file this run ends up opening. Beside the active one, a
    /// `--config D:\work\keys.toml` would leave it where the next plain start
    /// — which looks in `%APPDATA%` — never reads it.
    #[test]
    fn the_memory_sits_beside_the_default_config() {
        let Ok(default) = default_config_path() else {
            // No %APPDATA%: neither path can be built, and both say so the
            // same way rather than one of them guessing.
            assert!(memory_path().is_none());
            return;
        };
        let memory = memory_path().expect("a default config path means a memory path");
        assert_eq!(memory.parent(), default.parent());
        assert_eq!(
            memory.file_name().and_then(|name| name.to_str()),
            Some(winremap::config::last_used::FILE_NAME)
        );
    }

    /// The guarantee of ADR 0053: a shipped build does not know the flag. A
    /// misplaced `#[cfg]` would silently hand it to every user instead.
    #[test]
    #[cfg(not(feature = "test-inject"))]
    fn accept_injected_is_unknown_without_the_feature() {
        let Err(err) = parse(&["--accept-injected"]) else {
            panic!("--accept-injected must not be accepted outside a test build");
        };
        assert!(err.to_string().contains("--accept-injected"));
    }

    #[test]
    #[cfg(feature = "test-inject")]
    fn accept_injected_is_off_until_asked_for() {
        assert!(!parse(&[]).unwrap().accept_injected);
        assert!(parse(&["--accept-injected"]).unwrap().accept_injected);
    }
}
