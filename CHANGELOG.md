# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Project scaffolding: Cargo project, CI workflow, development docs (project brief, development plan, Rust guidelines), agent conventions (`AGENTS.md`).
- Core logic (M1): key notation parser (`C-h`, `Back`, ...), per-application keymap resolution with app-over-global priority, and TOML config loading with line-numbered validation errors. Config file format spec (`docs/04_config-spec.md`) and `examples/minimal.toml`.
- Win32 layer (M2): `WH_KEYBOARD_LL` hook with injected-event passthrough, `SendInput` sender with modifier lift/restore for exact-rule targets, foreground process name caching via `SetWinEventHook`, and a CLI entry point (`--config`, default `%APPDATA%\winremap\config.toml`).
- Resident features (M3): task tray icon (enable/disable toggle, config reload, open config, quit) via the `tray-icon` crate, hot reload with atomic table swap that keeps the previous config on errors, and single-instance guard via a named mutex.
- Release readiness (M4): `examples/emacs.toml` (fakeymacs-style bindings), README (en/ja), `SECURITY.md` with release verification steps, release workflow (exe + SHA256SUMS + build provenance attestation, draft release), CODEOWNERS, and owner runbook (`docs/06_release-operations.md`).
- Owner-designed keyboard icon (`assets/`), embedded as exe resources for both the tray (enabled/disabled states) and the executable itself.
- `exclude` lists for global keymaps (fakeymacs `not_emacs_target` equivalent), macro outputs (`"C-t" = ["C-Right", ...]`, up to 8 chords per press), and Emacs-style two-stroke prefix sequences (`"A-x u" = "C-z"`). New `examples/suganuma.toml` exercising all three.

- Japanese/English UI (tray, console messages, CLI help) auto-selected from the system language, with a `--lang en|ja` override.
- `--debug` flag: prints each foreground app's full path, the exact `application` value for the config, and the keymaps that would apply.

### Changed

- `examples/minimal.toml` now targets Notepad, which doubles as a quick way to verify winremap is active.
- Restructured `keymap`/`config` into folder modules with tests split into `tests.rs` (see guidelines §5).
