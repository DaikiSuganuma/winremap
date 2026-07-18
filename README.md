# winremap

[![CI](https://github.com/DaikiSuganuma/winremap/actions/workflows/ci.yml/badge.svg)](https://github.com/DaikiSuganuma/winremap/actions/workflows/ci.yml)

A per-application key remapper for Windows, written in Rust — inspired by
[xremap](https://github.com/xremap/xremap) (Linux) and
[Keyhac](https://github.com/crftwr/keyhac-win).

> winremap is an independent project influenced by Keyhac — not a
> reimplementation or fork of it. It is also not affiliated with xremap.

日本語版: [README.ja.md](README.ja.md)

## Features (v0.1)

- **Per-application remapping**: rules apply only to the processes you list
  (`phpstorm64.exe`, ...), with optional global (`*`) rules
- **Declarative TOML config** with Emacs-style key notation (`C-h`, `A-f`,
  `Back`, ...) familiar to Keyhac/fakeymacs users
- **Task tray resident**: enable/disable toggle, config hot-reload, quit
- **Single binary, no runtime dependencies**
- The hook callback runs in pure Rust with no heap allocation, locking, or
  I/O. Compared to script-driven remappers this improves worst-case latency
  and stability (no GC pauses that can get a low-level hook disconnected by
  Windows); average typing latency is similar

## Quick start

1. Download `winremap.exe` and `SHA256SUMS` from
   [Releases](https://github.com/DaikiSuganuma/winremap/releases)
   (see [SECURITY.md](SECURITY.md) for verification), or build from source:

   ```powershell
   cargo build --release   # -> target\release\winremap.exe
   ```

2. Create `%APPDATA%\winremap\config.toml` (or start with an example):

   ```toml
   # Ctrl+H sends a plain Backspace, but only inside PHPStorm
   [[keymap]]
   name = "jetbrains-terminal-fix"
   application = ["phpstorm64.exe"]

   [keymap.remap]
   "C-h" = "Back"
   ```

3. Run `winremap.exe`. A tray icon appears; remapping is active.

   ```powershell
   winremap.exe                     # uses %APPDATA%\winremap\config.toml
   winremap.exe --config my.toml    # explicit path
   ```

See [`examples/minimal.toml`](examples/minimal.toml) and
[`examples/emacs.toml`](examples/emacs.toml) (fakeymacs-style Emacs
bindings) for complete examples.

## Configuration

- `application` — exe names the section applies to (case-insensitive), or
  `["*"]` for all applications. App-specific rules always win over `*` rules.
- Key notation — modifiers `C-` (Ctrl), `A-` (Alt), `S-` (Shift), `W-` (Win)
  plus a key name: `a`-`z`, `0`-`9`, `F1`-`F24`, `Back`, `Enter`, `Esc`,
  `Tab`, `Space`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, arrow keys,
  `CapsLock`, and side-specific modifiers (`LCtrl`, ...) as outputs.
- A rule with modifiers (`"C-h" = "Back"`) matches that exact chord and
  replaces the modifier state too (the app receives a plain Backspace). A
  bare-key rule (`"CapsLock" = "LCtrl"`) swaps the key regardless of held
  modifiers.
- Config errors are reported with line numbers, all at once. Reloading a
  broken config from the tray keeps the previous working config.

The full specification lives in
[docs/04_config-spec.md](docs/04_config-spec.md) (Japanese).

## Limitations

- **Windows with elevated privileges** (admin) do not receive events from a
  non-elevated hook (UIPI). Run winremap elevated only if you need remapping
  there.
- **Punctuation/OEM keys** (`;`, `,`, ...) are not supported yet — their
  virtual-key codes are keyboard-layout dependent.
- **No key sequences** (`C-x C-c`), tap/hold, or mark mode yet (planned for
  v0.2 evaluation).
- Remapping chords that involve **Alt or Win** can momentarily trigger menu
  focus / Start menu due to the modifier lift, depending on the app.
- Games with anti-cheat and some virtualization software may ignore injected
  input.
- Do not run winremap together with other keyboard-hook software (Keyhac,
  AutoHotkey, ...) remapping the same keys — stacked low-level hooks have
  undefined ordering.
- winremap keeps a console window in v0.1 (reload errors are printed there).
- IME control is out of scope by design; use the Windows 11 IME settings.

## Security

- winremap **never logs or stores keystrokes** and contains **no network
  code** (no telemetry, no auto-update). The code base enforces this by
  policy; see [AGENTS.md](AGENTS.md).
- Official binaries are distributed **only** via
  [GitHub Releases](https://github.com/DaikiSuganuma/winremap/releases).
  Binaries obtained anywhere else are unofficial — verify checksums and
  build provenance as described in [SECURITY.md](SECURITY.md).

## Acknowledgments

- [Keyhac](https://sites.google.com/site/craftware/keyhac-ja) by craftware —
  the long-serving tool this project's workflow grew out of (MIT)
- [fakeymacs](https://github.com/smzht/fakeymacs) by smzht — Emacs-style
  keybinding configuration for Keyhac (MIT)
- [xremap](https://github.com/xremap/xremap) — the architectural reference
  for per-application remapping on Linux (MIT)

## License

[MIT](LICENSE) — Copyright (c) 2026 Daiki Suganuma
