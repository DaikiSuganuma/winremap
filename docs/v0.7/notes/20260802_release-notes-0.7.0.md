## WinRemap v0.7.0 — 記号キーが設定に書けるようになります

`;` や `/` や `@` を、ようやくルールに書けます。README の「できないこと」から 1 項目消える版です。

### 新機能

- **記号キーをリマップできます**
  `"C-;" = "Enter"` のように、**お使いのキーボードの刻印どおり**に書きます。WinRemap は表を持たず、そのつど Windows に聞きます — 同じ `0xC0` が日本語 106 キーボードでは `@`、US 101 では `` ` `` だからです。表を 1 つ持てば、**利用者の半分にとって嘘になります**。
  - `Oem1`〜`Oem102` という**配列によらない名前**も使えます。文字では指せないキーが実在するためです（何も刻まれていないキー、`\` を出すキーが 2 つある配列）
  - **Shift を押さないと打てない文字は、そのままでは書けません。** `"C-@"` は US 配列ではエラーになり、`` `@` needs Shift on this keyboard; write `S-2` instead `` と**代わりの綴りを案内します**。黙って `S-` を補うと、**同じ設定ファイルが機械によって別のコマンドを意味する**ことになるためです
  - 解決は設定を読み込むときに 1 回だけ行います。**キーボードを差し替えたら、トレイの「再読み込み」を押してください**

- **設定ウィンドウの「使えるキー名」が、そのキーボードの刻印を並べます**
  固定の一覧ではなく、**つないである実物**から読んでいます。

- **`C--`（Ctrl とマイナスキー）が書けるようになりました**
  `-` は修飾キーの区切りと同じ文字なので、パーサーが「修飾キー名が空」と読んで弾いていました。記号キー対応以前からあった不具合です。

### 修正

- **切り替えた先のアプリのルールが、ちゃんと適用されます**
  前面アプリの監視が、イベントの伝える「前面になったウィンドウ」ではなく「今どれが前面か」を Windows に聞き直していました。切り替えの最中に聞くと**直前まで前面だったウィンドウ**が返ることがあり、実測で**5 回に 1 回ほど**、次にアプリを切り替えるまで**1 つ前のアプリのキーマップが使われていました**。アプリ別のルールが黙って効かず、ログの前面アプリ行も間違ったアプリを指していました。イベントが正しいウィンドウを運んでくるので、聞き直しをやめました。

- **ログが記号キーに生のコードを併記しなくなりました**（`/ (0xBF)` → `/`）
  括弧は「このキーはまだ設定に書けない」という合図でもありました。書けるようになった以上は嘘になります。**ログに出た綴りは、そのままルールに貼れます。** まだ書けないキーには従来どおりコードが付きます。

### インストール

- **Microsoft Store**: https://apps.microsoft.com/detail/9N6TQDXRX5WV （警告なし・自動更新）
- インストーラー: `winremap-setup.exe`（管理者権限不要・ユーザー単位）
- ポータブル: `winremap.exe`（1 ファイル。設定は `%APPDATA%\winremap\config.toml`）

`SmartScreen`（「Windows によって PC が保護されました」）は**GitHub から落としたファイル**に出ます。署名なしのためです。**詳細情報 → 実行**で起動できます。下記の検証を推奨します。Store 版ではこの警告自体が出ません。

### ダウンロードの検証

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # SHA256SUMS と照合
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

公式の配布経路は **Microsoft Store と GitHub Releases の 2 つ**です。それ以外で配布されているバイナリは非公式です。

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v0.7.0/CHANGELOG.md

---

## WinRemap v0.7.0 — Punctuation keys, at last

`;`, `/` and `@` can finally go in your rules. This release removes an item from the Limitations list in the README.

### New

- **Symbol and punctuation keys work in rules.**
  Write them **the way your keyboard is engraved** — `"C-;" = "Enter"`. WinRemap carries no table and asks Windows instead, because the same `0xC0` prints `@` on a Japanese 106 keyboard and `` ` `` on a US 101. One fixed table would be **a lie to half its users**.
  - The **layout-independent names** `Oem1`–`Oem102` are accepted too, because some keys cannot be reached by character at all: a key that prints nothing, and layouts where two different keys both print `\`
  - **A character that needs Shift cannot be written as itself.** On a US layout `"C-@"` is an error that tells you the spelling to use: `` `@` needs Shift on this keyboard; write `S-2` instead ``. Silently adding the `S-` would make **the same config file mean different chords on different machines**
  - Keys are resolved once, when the config is loaded. **Swap keyboards and you need "Reload config" from the tray**

- **The settings window's key-name popup lists the symbols your keyboard prints**, read from the hardware rather than from a list.

- **`C--` (Ctrl and the minus key) can be written at all.** `-` is also the modifier separator, so the parser read it as an empty modifier name and rejected it. That predates symbol keys.

### Fixed

- **Rules apply to the application you switched to.**
  The foreground watcher asked Windows which window was in front, rather than using the one the event named. Asked mid-switch, that answer can be **the window you just left** — measured, about **one switch in five** kept resolving keys against the previous application until you switched again. Application-scoped keymaps silently did nothing, and the log's foreground line named the wrong app. The event carries the right window, so nothing has to be asked.

- **The log no longer prints a raw code beside a symbol key** (`/ (0xBF)` is now `/`).
  The parentheses also meant "this key cannot be written in the config yet", which is no longer true. **What the log shows can be pasted straight into a rule.** Keys that still cannot be written keep their code.

### Install

- **Microsoft Store**: https://apps.microsoft.com/detail/9N6TQDXRX5WV (no warning, automatic updates)
- Installer: `winremap-setup.exe` (per-user, no admin rights needed)
- Portable: `winremap.exe` (one file; config at `%APPDATA%\winremap\config.toml`)

`SmartScreen` ("Windows protected your PC") applies to **files downloaded from GitHub**, because those are unsigned. **More info → Run anyway**, and verifying your download is recommended. The Store version never shows it.

### Verify your download

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # compare with SHA256SUMS
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

There are **two** official channels: the Microsoft Store and GitHub Releases. Binaries distributed anywhere else are unofficial.

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v0.7.0/CHANGELOG.md
