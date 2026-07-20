# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Tray menu item **Show log** (ADR 0029): opens a window that streams the debug log live, so diagnosing a keymap no longer requires starting WinRemap from a terminal. Debug logging is on only while the window is open, the log is never written to disk, and the window runs on its own thread so remapping is unaffected. Built with egui (ADR 0030), which also covers the v0.2 config GUI. Closing the window hides it and keeps its event loop alive — winit allows only one per process — so it can be reopened any number of times (ADR 0032). It carries the WinRemap icon, and config reloads and error messages show up in it as well.
- The tray menu now opens with a disabled caption line showing the app name and version (`WinRemap v0.1.0`).

### Fixed

- IME indicator: the panel never appeared in the Windows 11 Notepad (ADR 0033). Notepad is a WinUI 3 app whose editor runs on a second UI thread, and the IME open status is per thread, so querying the foreground window always read OFF. The status is now asked of every input thread of the foreground app, which also subsumes the UWP CoreWindow special case from ADR 0023.
- Help site on GitHub Pages (ADR 0028): a single-page user guide (English and Japanese) covering install, quick start, configuration reference, IME indicator, and troubleshooting, deployed from `site/` via GitHub Actions.

## [0.1.0] - 2026-07-20

### Added

- Project scaffolding: Cargo project, CI workflow, development docs (project brief, development plan, Rust guidelines), agent conventions (`AGENTS.md`).
- Core logic (M1): key notation parser (`C-h`, `Back`, ...), per-application keymap resolution with app-over-global priority, and TOML config loading with line-numbered validation errors. Config file format spec (`docs/v0.1/02_config-spec.md`) and `examples/minimal.toml`.
- Win32 layer (M2): `WH_KEYBOARD_LL` hook with injected-event passthrough, `SendInput` sender with modifier lift/restore for exact-rule targets, foreground process name caching via `SetWinEventHook`, and a CLI entry point (`--config`, default `%APPDATA%\winremap\config.toml`).
- Resident features (M3): task tray icon (enable/disable toggle, config reload, open config, quit) via the `tray-icon` crate, hot reload with atomic table swap that keeps the previous config on errors, and single-instance guard via a named mutex.
- Release readiness (M4): `examples/emacs.toml` (fakeymacs-style bindings), README (en/ja), `SECURITY.md` with release verification steps, release workflow (exe + SHA256SUMS + build provenance attestation, draft release), CODEOWNERS, and owner runbook (`docs/06_release-operations.md`).
- Owner-designed keyboard icon (`assets/`), embedded as exe resources for both the tray (enabled/disabled states) and the executable itself.
- `exclude` lists for global keymaps (fakeymacs `not_emacs_target` equivalent), macro outputs (`"C-t" = ["C-Right", ...]`, up to 8 chords per press), and Emacs-style two-stroke prefix sequences (`"A-x u" = "C-z"`). New `examples/suganuma.toml` exercising all three.

- Japanese/English UI (tray, console messages, CLI help) auto-selected from the system language, with a `--lang en|ja` override.
- `--debug` flag: prints each foreground app's full path, the exact `application` value for the config, and the keymaps that would apply.

- `--debug` now also logs each key decision (pass-through / remap / macro / prefix / swallowed) at key-name level, queued lock-free from the hook and printed on the message loop.
- Richer `--debug` output for diagnosing macros: the macro's element list, an echo of every injected event passing the hook (ours labeled remap/modifier-adjust, foreign software labeled EXTERNAL), and suppressed auto-repeats.
- `--macro-delay <ms>` (0-15, default 0): opt-in pacing between macro strokes for apps that mishandle burst-injected input.
- Top-level `macro_delay_ms` config option (CLI `--macro-delay` overrides it), applied on reload too; `examples/suganuma.toml` sets 8 ms, confirmed to stabilize macros in the WinUI Notepad.
- IME status indicator (`[ime_indicator]`, opt-in, ADR 0020-0022): flashes a translucent "あ" panel at the center of the active window the moment the IME turns on (or a focused window's IME is on), fading out after `duration_ms`. Detection combines standard IME toggle keys with configurable `trigger_keys` (e.g. `["C-Space"]`) plus foreground-change checks via `IMC_GETOPENSTATUS`; runs on a dedicated thread so remapping is never affected. Display only — WinRemap never switches the IME. Shell surfaces (taskbar, desktop, tray-overflow, and input-switcher flyouts) never show the panel — but returning from them to an app whose IME is on re-flashes it — UWP apps (Settings, ...) are queried through their CoreWindow child, and `show_app_name = true` adds the target app's exe name under the glyph. Ships with the `ime_probe` example (status polling and `--overlay` visual self-test) and `--debug` query diagnostics.
- Windows installer `winremap-setup.exe` (Inno Setup, ADR 0027): per-user install requiring no admin rights, English/Japanese installer UI, Start Menu shortcut, optional start-at-sign-in, and a starter config created from `examples/minimal.toml` only when `%APPDATA%\winremap\config.toml` does not exist yet. The portable single exe remains available; both artifacts are covered by `SHA256SUMS` and the build-provenance attestation.

### Changed

- **No console window on startup** (ADR 0029): WinRemap is now a windows-subsystem binary, so launching it from Explorer, the Start menu, or the sign-in autostart entry no longer flashes a console. Started from a terminal it attaches to that terminal and prints as before (`--debug`, `--help`, `--version`), and redirects like `winremap --help > out.txt` keep working. Messages that must not be missed — a startup failure, a failed config reload — become a dialog when there is no terminal to print to.

- The product name is written **WinRemap** in documentation and UI strings (matching the WinMerge/WinSCP naming convention); technical identifiers — the crate, `winremap.exe`, `%APPDATA%\winremap\`, repository URLs — stay lowercase (ADR 0025).
- `--debug` logs pass-through keys once per physical press: auto-repeats of keys WinRemap does not remap (e.g. a held push-to-talk key) no longer flood the log.
- `examples/minimal.toml` now targets Notepad, which doubles as a quick way to verify WinRemap is active.
- Restructured `keymap`/`config` into folder modules with tests split into `tests.rs` (see guidelines §5).
- `examples/suganuma.toml` comments are now in Japanese.

### Fixed

- Alt/Win chords (e.g. the `A-a` select-all macro, `A-x` prefixes) no longer trigger the menu bar / Start menu: a masking key tap is injected around Alt/Win transitions, and consumed chords mask the physical modifier release too.
- Macros fired intermittently in apps that sample modifier state asynchronously (e.g. the new Notepad): modifier events are now emitted as minimal diffs between macro elements instead of a full lift/re-press per element, so e.g. `C-t` never touches the physically held Ctrl at all.
