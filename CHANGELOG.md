# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **The settings window can edit the IME cursor colour.** `change_cursor_color`
  and `cursor_color` were in the README, the help site and the config spec, but
  nowhere in the settings window — and not merely missing from the display: the
  draft the window edits did not carry the two keys at all, so they could not be
  read, changed or saved there. They are now under `[ime_indicator]`, and they
  are shown **whether or not `enabled` is ticked**, because the cursor is
  independent of the panel (ADR 0067) — hiding it behind that tick would undo on
  screen what 0.8.0 decided.

  Nothing was ever lost by the omission: the window rewrites only the keys whose
  draft value changed, so keys it did not know about were left alone. That is
  luck rather than design, and there is now a test that holds it.

### Fixed

- **A cursor with nothing drawn in it is never installed** (ADR 0073). Rarely —
  four times during 0.8.0's acceptance, never on demand — the tinted I-beam
  stopped being drawn at all until WinRemap was restarted. Measured while it was
  happening, the cursor Windows was holding turned out to be one WinRemap had
  built and it was **completely empty**: 1024 of 1024 pixels transparent. It
  then survived every attempt to clear it because an empty cursor was used as
  the source for the next one, which produces another empty cursor — that is why
  toggling the IME never brought it back. Both ends are now checked. A cursor
  with no drawn pixels is not used as a source and not installed as a result,
  and either refusal is written to the log with the reason.

  **The cause of the first empty cursor is still unknown.** Six explanations
  were measured and ruled out, and it has never been reproduced on demand. This
  change stops the symptom reaching you and makes the next occurrence say so in
  `--debug` instead of passing in silence.
- **A tint that only half went on now says so** (ADR 0075). WinRemap recolours
  two cursors, the arrow and the I-beam. When one of them could not be built it
  was quietly dropped, and the log reported the other one going on as a
  success — *tint installed — 1 replaced*, no failure named. During 0.9.0's own
  acceptance that is exactly what happened: the I-beam went unbuilt at startup
  and the arrow alone carried the colour for twenty minutes, with nothing
  anywhere saying why. The count is now taken as "everything that did not go
  on", so no path can slip past it, and the reason a cursor is missing is
  repeated each time the IME is switched on rather than said once, at startup,
  into a log nobody had opened yet.

  **This is what 0.9.0 is for.** The empty cursor above is refused, as
  designed — but the refusal has to be legible, and it was not.
- **Restoring the cursor works now.** It was written in two stages, and the
  first one — reloading the stock shapes out of `user32.dll` — **had never once
  run since the feature shipped in 0.8.0**. Windows 11 answers
  `ERROR_RESOURCE_NAME_NOT_FOUND` for those resource ids, and the failure was
  discarded without a word. What was left was the second stage,
  `SystemParametersInfo(SPI_SETCURSORS)`, which reloads from the cursor scheme
  in the registry — empty on the stock scheme, which is the very case the first
  stage was added for. WinRemap now copies the stock cursors at startup, before
  it has changed anything, and restores from those copies; `user32.dll` is no
  longer asked, because its resource ids are undocumented and may move again.
  Startup reports how many copies it holds, so whether the safety net is up can
  be looked at rather than assumed.
- **The Microsoft Store build shows its own icon.** It was a solid blue square —
  on the taskbar, on the window, and in Settings → Apps → Startup. Three faults
  overlapped. The package carried **no resource index**, so the differently
  sized and unplated images inside it were never consulted at all and only the
  single file the manifest names was ever used; the 44-pixel unplated image that
  the official procedure asks for was missing; and the plate behind the logo was
  `transparent`, which means "use the accent colour" rather than "no plate", so
  a blue logo sat on a blue plate. The package now carries an index, and the
  build reads it back and stops if the unplated images are not in it — putting
  a file in a package is not the same as it being used.

  **This has been wrong since 0.6.0**, and 0.8.0 was submitted before it was
  found, so 0.9.0 is the first Store build with the icon right. Builds from
  GitHub Releases were never affected: the installer's icon is embedded in the
  executable and does not go through the index.

## [0.8.0] - 2026-08-09

### Added

- **The mouse cursor can show whether the IME is on** (ADR 0067).
  `change_cursor_color = true` under `[ime_indicator]` draws the arrow and the
  text I-beam in `cursor_color` (default `#0078d4`) with a white border for as
  long as the IME is on. It is independent of `enabled`, so you can have the
  cursor without the panel, and unlike the panel it is not a flash you can
  miss — it is there whenever you look. Your own cursor theme and size are
  kept: the shape is recoloured, not replaced. The white border is not
  decoration; the colour alone is 37% as bright as the white it replaces and
  would be lost on a dark application, so the cursor is given two colours at
  opposite ends of the brightness range.

  This changes the cursor **for the whole session**, not just over WinRemap's
  own windows, which is why it is off by default. It also means a cursor that
  is still coloured with no WinRemap in the tray is WinRemap having been
  killed — nothing to repair, and starting it again puts the cursor back.

  Two limits worth knowing. It cannot follow the IME while an elevated window
  is in front, for the same reason remapping cannot reach one (UIPI). And
  rarely — three times on one machine during acceptance, never on demand —
  the coloured I-beam stops being drawn at all until WinRemap is restarted;
  the cause is not yet known, and `--debug` now records every cursor change so
  a report can identify it.
- **`--debug` opens a console window of its own** and keeps it after WinRemap
  exits (ADR 0068). The log used to go to the terminal you started from, where
  the shell's prompt repainted over it, and it could only be read while
  WinRemap was alive — so the startup lines had scrolled past before the log
  window could be opened, and the shutdown lines vanished with the process.
  Now both can be read: the window is there from the first line of startup and
  stays until you close it or press Enter. Closing that window while WinRemap
  is running ends WinRemap, which is what the flag is for; the terminal you
  launched from is free to close. Redirected output still goes to the
  destination and opens no window.

  `--help` and `--version` are deliberately unchanged: they still print to the
  terminal you started from, where the shell's prompt may repaint over them.

## [0.7.0] - 2026-08-02

### Added

- **Punctuation and symbol keys can be used in rules** (ADR 0063). `"C-;"`,
  `"C-/"` and the rest now work, written the way your keyboard is engraved —
  WinRemap asks Windows what each key prints rather than carrying a table,
  because `0xC0` is `@` on a Japanese 106 keyboard and `` ` `` on a US 101.
  The fixed names `Oem1`–`Oem102` name the same keys on any layout, for the
  two cases a character cannot reach: a key that prints nothing, and the two
  different keys that both print `\`. A character that needs Shift on your
  keyboard is refused with the spelling to use instead — `` `@` needs Shift
  on this keyboard; write `S-2` instead `` — rather than being silently
  rewritten, because a rule that means two different chords on two machines
  is worse than an error. Keys are resolved once when the config is compiled,
  so **swapping keyboards means reloading the config**.
- The settings window's key-name popup lists the symbols **this** keyboard
  prints, read from the keyboard rather than from a list.
- `C--` — the minus key with Ctrl — can be written at all. The parser read it
  as an empty modifier name and rejected it, which predates symbol keys.

### Fixed

- **The app you switch to is the one your rules apply to** (ADR 0065). The
  foreground watcher asked Windows which window was in front instead of using
  the one the event named, and that answer can still be the window you just
  left: about one switch in five resolved keys against the previous
  application until you switched again. Application-scoped keymaps silently
  did nothing, and the log's foreground line named the wrong app. The event
  carries the right window, so nothing has to be asked.
- The log no longer prints a raw code beside a symbol key (`/ (0xBF)`). The
  parentheses meant "this key cannot be written in the config yet", which is
  no longer true — what the log shows can now be pasted straight into a rule.
  Keys that still cannot be written keep the code.

## [0.6.0] - 2026-07-29

### Added

- **Microsoft Store packaging** (ADR 0060). WinRemap can be built as an MSIX
  package, which the Store re-signs — so installing it from there shows no
  "Windows protected your PC" warning at all. Downloads from GitHub Releases
  are unaffected and still warn; the binaries there remain unsigned.
- Screenshots of the settings and log windows in both READMEs, and the Store
  as a second install route throughout the docs: SECURITY.md now describes
  two official channels rather than one, the install guide compares them, and
  the FAQ answers what actually differs — including where each one keeps your
  config file. Every page of the help site now links the privacy policy.

### Fixed

- **Closing the terminal you started WinRemap from no longer closes WinRemap**
  (ADR 0062). It attaches to that terminal's console so `--debug` output has
  somewhere to go, and Windows kills every process attached to a console when
  its window closes — so a normal launch now hands the console back as soon as
  remapping is live. `--debug` still streams to the terminal and still ends
  with it, which is what that flag is for.
- **WinRemap starts on a machine that has no config file yet** (ADR 0059). It
  used to refuse, which was invisible only because the installer seeded
  `%APPDATA%\winremap\config.toml` for you — so the portable exe failed on
  first run, and so would a Microsoft Store package, which has no
  install-time step at all. The default config is now written on first run
  (the same Notepad example the installer places) and the session says where
  it landed. A path given with `--config` still has to exist: a typo there
  should say so, not quietly produce a WinRemap that remaps nothing. An
  existing config is never overwritten.
- **The Store build reports a config path that exists** (ADR 0061). A packaged
  app has its `%APPDATA%` writes redirected somewhere private, which is
  invisible to WinRemap itself but not to the programs it hands the path to:
  "open in text editor" and "open folder" pointed at a location that is not
  there for Explorer or your editor. The path is now resolved once at
  startup, so everything — the address bar, the file watch, both links —
  agrees on one location. A config carried over from an installed WinRemap
  keeps being used rather than being replaced by a fresh default.
- The help site linked `examples/suganuma.toml`, which was renamed to
  `personal-ja.toml` in 0.5.0 — the link 404'd on both language versions.

## [0.5.0] - 2026-07-29

### Added

- **The log window says what a key sends.** Where a key or a chord carries an
  ASCII control code, the line names it: `C-h (BS 0x08) → remapped to Back
  (BS 0x08)`. That pair is the problem WinRemap was written for — Ctrl+H and
  Backspace look alike and behave differently in a terminal — and until now
  the log named both keys and said what neither one sends (ADR 0056).
  Letters and digits never show a code: WinRemap logs keys, not what you
  typed, and still writes nothing to disk.
- **Two views of the log.** The default is one line per key, saying what
  WinRemap decided. Tick **Every event** for the whole stream: every physical
  press and release, and everything WinRemap sent in reply — including the
  modifier lifted before a remap target and put back when you let go, which
  happens at a different moment and now says so. Every line is recorded
  either way, so ticking the box explains the keys you already pressed rather
  than only the next ones (ADR 0057).
- **Keys the config cannot name yet are named anyway.** `Num1 (0x61)` instead
  of `0x61`, and for punctuation the character your own keyboard layout
  prints on the key — asked of Windows rather than assumed, since `0xC0` is
  `@` on a Japanese layout and a backquote on a US one (ADR 0058).
- **The log window says which config file is loaded**, on the line under the
  session banner. A reload said which file it had read while startup never
  did, and the window is usually opened long after that line scrolled past.
- **The settings and log windows are now readable by assistive technology.**
  Both are exposed through UI Automation via AccessKit, which also lets the
  project's UI tests read and press what a person sees instead of matching
  screenshots (ADR 0055).

### Changed

- **`--debug` now decides whether anything is printed to a terminal.** It
  used to print whenever one was attached, so starting WinRemap from a shell
  produced a running commentary nobody asked for; without the flag it is now
  as quiet as a launch from Explorer. With the flag, the terminal gets
  exactly what the log window shows — same stamps, same tags — because both
  are now formatted in one place (ADR 0058). Errors and `--help` are not log
  output and print as before.
- **The log is readable at a glance.** Every line carries the time to the
  millisecond and a tag saying which stream it belongs to (`[input]`,
  `[decided]`, `[injected]`, `[action]`, `[window]`, `[IME]`) in place of the
  `[debug]` that led every line and distinguished nothing. A stamp equal to
  the one above it is left blank, so everything WinRemap did in reply to one
  press reads as a group. The report for the application in front is one
  line naming the `application` value to write and the keymaps it reaches,
  with the path underneath, instead of three untimed lines that read as part
  of the previous keystroke. The window opens larger (900×720).
- **The log window's own controls leave a line.** Follow newest, Every event,
  Clear and Copy all each record what they did, so "why did the log stop
  moving" and "why did those lines disappear" are answered by the log.

## [0.4.0] - 2026-07-26

### Added

- **Config editing in the settings window** (milestone B2, ADR 0049): the
  Edit button turns the window into an editor for the config file itself —
  keymap names, target apps, exclusions and remap rules as editable cells,
  keymaps addable/deletable/reorderable from the tree, and the general
  settings on sliders and checkboxes. Save validates with the same parser
  the CLI and tray use, writes atomically, and reloads; comments, blank
  lines, ordering and your own spellings survive untouched (ADR 0036).
  A file changed outside WinRemap is never silently overwritten, a failed
  save keeps the draft, and closing with unsaved changes asks first.
- **Explorer-style settings window**: an address bar with a dropdown over
  the config folder's `.toml` files — switch the active config by picking
  one (ADR 0050); a ● marks any file that changed on disk, watched via the
  `notify` crate while the window is open (ADR 0051). An icon tree with
  full-row selection replaces the plain list, a breadcrumb tops the detail
  pane, and a status bar shows the version, when WinRemap started, and the
  last thing that happened.

### Changed

- The example configs now open with the same header — what a config file is,
  and a short example of every form a rule can take — and every rule carries a
  one-line comment saying what it does. `examples/suganuma.toml` is now
  `examples/personal-ja.toml`, with its prose in Japanese.
- `examples/emacs.toml` gained the `C-x` map — `C-x C-s` to save, `C-x u` to
  undo, `C-x h` to select all, and four more — using the two-stroke sequences
  WinRemap has supported since v0.1 (ADR 0013).
- Deleting a remap rule now keeps the comment *lines* above it and moves them
  down to the next rule, whatever the blank lines around them look like — only
  the rule's own line and the comment written on it go (ADR 0054). A heading
  like `# --- Editing ---` used to disappear along with the first rule under
  it. Rules the editor writes are quoted the way the ones around them are
  (`"C-n" = "Down"`), and a comment shown in the settings window no longer
  carries a `#`: it is a note there, not a line of TOML.
- The application list of a keymap set to `*` no longer offers to add, capture
  or widen anything — every application already matches — and a keymap that
  names its apps gets a "Target all applications" button instead. The
  exclusion list is now headed "Excluded applications" and carries the same
  foreground-capture button the application list has.

### Fixed

- Deleting the last keymap of a config no longer empties the file. The comment
  block a file opens with belonged to the first `[[keymap]]`, so removing the
  only one took the whole header with it.
- Saving no longer rewrites a config file's line endings. A file written with
  Windows line endings came back with every line changed, so a one-word edit
  showed up as a whole-file diff.
- The comment block at the top of a config file that starts with `[[keymap]]`
  now stays at the top. It belonged to whichever keymap happened to be first,
  so reordering the keymaps carried the file's own header along with one of
  them, and deleting that keymap deleted the header.
- A config that fails to load now says so in the settings window's status bar
  as well. Switching to a broken file left the bar reading "Loaded", with the
  reason on the terminal WinRemap was started from — which reads as success.
- The settings window no longer rebuilds its panels halfway through a frame
  when a footer button is pressed, which made debug builds outline the window
  in red for a moment.
- The executable no longer depends on `vcruntime140.dll`, so it starts on a
  clean Windows machine without the VC++ redistributable installed. This is
  what crashed winget's validation of v0.3.0 with `STATUS_DLL_NOT_FOUND`;
  the C runtime is now linked statically (ADR 0052).

## [0.3.0] - 2026-07-22

### Added

- **Macro recording**: press the recording key, do the work, press it again,
  and the replay key repeats it (ADR 0043). What gets recorded is what
  WinRemap *emitted* — a remapped chord, each command of a macro, the key
  itself where no rule matched — so a replay is indistinguishable from
  having typed it. Configured under `[macro]` as `record_start`,
  `record_stop` (omit it and the start key toggles) and `record_play`, and
  off entirely until you name those keys. A recording holds 20 commands and
  says so on screen while it runs; reaching the limit ends the recording and
  tells you rather than dropping commands quietly. The banner sits on the
  display holding the app you are typing into and follows you between apps
  and monitors, naming that app and repeating the keys that stop and replay
  the recording — so it cannot be missed, and stopping never means going
  back to the config file. **Nothing is written to
  disk**: the recording lives in memory and is gone when WinRemap exits.
  Replay runs on its own thread, never in the keyboard hook, because 20
  commands at the maximum pacing would reach the timeout Windows applies to
  low-level hooks and cost you the hook itself (ADR 0044).

## [0.2.0] - 2026-07-21

The theme of this release is **seeing what WinRemap is doing**. Launching no
longer flashes a console window, and two new windows — a live log and a
settings viewer — answer the questions that previously meant reading the
config file and guessing.

Editing the config from the GUI is not in this release; the settings window is
read-only for now. Writing to a config file people hand-wrote deserves more
care than the schedule allowed, and the viewer is useful on its own.

### Added

- **Settings window** on the tray menu, showing the config that is in effect
  right now — the very table the hook resolves against, not a re-read of the
  file. Keymaps down the left; the selected one's target apps, exclusions and
  remap rules on the right, each rule with the comment written beside it in
  the file. A key-notation legend sits in its own pane alongside the rules.
  Where the same input is bound in more than one keymap, a column names the
  others: only one can win, and that is invisible when reading either alone.
  The file's modification time and the load time are shown side by side, with
  a reload button next to them, and **Open in text editor** hands the file to
  whatever you have associated with `.toml` — which is where the old **Open
  config file** menu item went.
- Tray menu item **Show log** (ADR 0029): opens a window that streams the debug log live, so diagnosing a keymap no longer requires starting WinRemap from a terminal. Debug logging is on only while the window is open, the log is never written to disk, and the window runs on its own thread so remapping is unaffected. Built with egui (ADR 0030), which also covers the v0.2 config GUI. Closing the window hides it and keeps its event loop alive — winit allows only one per process — so it can be reopened any number of times (ADR 0032). It carries the WinRemap icon, and the tray's enable/disable toggle, config reloads, and error messages show up in it as well.
- The tray menu now opens with a disabled caption line showing the app name and version (`WinRemap v0.2.0`).
- Tray menu icons from [Bootstrap Icons](https://icons.getbootstrap.com/) (MIT), rasterized from SVG at build time so nothing of the rasterizer ships in the binary (ADR 0040). The caption row carries the app's own icon; the enable/disable toggle keeps its checkmark and no icon.
- `THIRD-PARTY-NOTICES.md`, carrying the Bootstrap Icons copyright and MIT permission notice. The rasterized icons are embedded in `winremap.exe`, so the notice now ships with the binary: it is a release asset and the installer puts it beside the exe.
- All WinRemap windows share one event loop, since winit allows only one per process (ADR 0035). An invisible off-screen host owns it and both real windows are its children (ADR 0037), which is what lets either be opened, closed and reopened independently.
- Help site on GitHub Pages (ADR 0028): a single-page user guide (English and Japanese) covering install, quick start, configuration reference, IME indicator, and troubleshooting, deployed from `site/` via GitHub Actions.
- The log records what you did, not just what the keyboard did: tray picks, window opens and closes, and reload requests are marked `[action]`. It opens with the launch time and version, and the console gets a matching line on exit.

### Changed

- **WinRemap no longer opens a console window.** It is now a windows-subsystem binary (ADR 0029), so launching from Explorer, the Start menu or the sign-in autostart entry is silent. Started from a terminal it attaches to that terminal, so `--debug`, `--help` and `--version` still print where you ran them; without one, anything you must not miss (a config error, an unknown argument) becomes a dialog rather than vanishing.
- Macro pacing moves from the top-level `macro_delay_ms` to `[macro]` `delay_ms` (ADR 0039), so it sits in a section like `[ime_indicator]` does. **The v0.1 spelling still works** — setting both is a validation error rather than a silent precedence.

### Fixed

- IME indicator: the panel never appeared in the Windows 11 Notepad (ADR 0033). Notepad is a WinUI 3 app whose editor runs on a second UI thread, and the IME open status is per thread, so querying the foreground window always read OFF. The status is now asked of every input thread of the foreground app, which also subsumes the UWP CoreWindow special case from ADR 0023.

## [0.1.0] - 2026-07-20

### Added

- Project scaffolding: Cargo project, CI workflow, development docs (project brief, development plan, Rust guidelines), agent conventions (`AGENTS.md`).
- Core logic (M1): key notation parser (`C-h`, `Back`, ...), per-application keymap resolution with app-over-global priority, and TOML config loading with line-numbered validation errors. Config file format spec (`docs/v0.1/02_config-spec.md`) and `examples/minimal.toml`.
- Win32 layer (M2): `WH_KEYBOARD_LL` hook with injected-event passthrough, `SendInput` sender with modifier lift/restore for exact-rule targets, foreground process name caching via `SetWinEventHook`, and a CLI entry point (`--config`, default `%APPDATA%\winremap\config.toml`).
- Resident features (M3): task tray icon (enable/disable toggle, config reload, open config, quit) via the `tray-icon` crate, hot reload with atomic table swap that keeps the previous config on errors, and single-instance guard via a named mutex.
- Release readiness (M4): `examples/emacs.toml` (fakeymacs-style bindings), README (en/ja), `SECURITY.md` with release verification steps, release workflow (exe + SHA256SUMS + build provenance attestation, draft release), CODEOWNERS, and owner runbook (`docs/03_release-operations.md`).
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
