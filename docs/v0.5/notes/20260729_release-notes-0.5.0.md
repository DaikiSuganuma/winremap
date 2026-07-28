## WinRemap v0.5.0 — ログが、何が起きたかを言うようになりました

キー名だけを並べていたログが、**いつ・何が届き・WinRemap が何をして・何を送ったか**を出すようになりました。

### 新機能

- **キーが何を送るかを表示します**
  ASCII 制御コードを持つキー・コマンドは、それも出ます。

      18:01:51.517 [判定]   C-h (BS 0x08) → Back (BS 0x08) に置換

  この組み合わせが WinRemap を作るきっかけでした。Ctrl+H と Backspace は同じに見えて、端末では違う動きをします。**英数字にはコードを付けません** — WinRemap が記録するのはキーであって、打った内容ではないからです（ログはディスクにも保存されません）。
  - 端末は Backspace キーに DEL `0x7f`、Ctrl+H に BS `0x08` を送り、アプリがその 2 つに別の動作を割り当てていることがあります。`C-h` を `Back` にリマップすると、端末はどちらでも `0x7f` を送るようになります。ログで両側とも `BS 0x08` と出るのは、それが **Windows が Backspace キーに与えているコード**だからです

- **ログに 2 つの表示モード**
  既定はキー 1 つにつき 1 行の判定です。［全イベント］にチェックを入れると、流れた入力をすべて出します — 物理キーの押下・解放と、それに対して WinRemap が送出したイベントの全部です。**リマップは 2 つの時刻に分かれて起きます**（置換先はキーを押した時に押され、離した時に離されます）。記録はチェックの有無にかかわらず取っているので、**入れる前に押したキーの詳細も見えます**。

- **読みやすくしました**
  各行にミリ秒までの時刻と、どの流れの行かを示すタグ（`[入力]`・`[判定]`・`[注入]`・`[操作]`・`[前面]`）が付きます。直前と同じ時刻は空欄になるので、1 回の押下に対する数行が 1 かたまりに見えます。前面アプリの報告は、`application` に書くべき値と適用されるキーマップを 1 行で出す形になりました。ウィンドウは初期状態で大きく開きます。

- **キーの名前が出ます**
  設定ファイルにまだ書けないキーも、`0x61` ではなく `Num1 (0x61)` と出ます。記号キーは、**あなたのキーボードの配列が刻んでいる文字**で出ます（`0xC0` は日本語配列では `@`、US 配列では `` ` `` です）。

- **ログウィンドウが、読み込んでいる設定ファイルを名乗ります。** 再読み込みしたときだけ出ていたものが、開いた時点で分かるようになりました。

- **設定ウィンドウとログウィンドウをスクリーンリーダーが読めるようになりました**（UI Automation）。

### 変更

- **`--debug` が、端末に出すかどうかを決めるようになりました。** これまでは端末から起動しただけで実況が流れていました。オプションを付けなければ、シェルから起動してもエクスプローラーから起動したときと同じように**何も出しません**。付けた場合は、**ログウィンドウと同じ内容**（時刻・タグ付き）がその端末に出ます。エラーと `--help` はログではないので従来どおり出ます。

### インストール

- インストーラー: `winremap-setup.exe`（管理者権限不要・ユーザー単位）
- ポータブル: `winremap.exe`（1 ファイル。設定は `%APPDATA%\winremap\config.toml`）

`SmartScreen`（「Windows によって PC が保護されました」）が出たら、署名なしのためです。**詳細情報 → 実行**で起動できます。下記の検証を推奨します。

### ダウンロードの検証

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # SHA256SUMS と照合
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

GitHub Releases 以外で配布されているバイナリは非公式です。

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v0.5.0/CHANGELOG.md

---

## WinRemap v0.5.0 — The log now says what happened

A log that listed key names now tells you **when something arrived, what WinRemap decided, and what it sent in reply**.

### New

- **The log says what a key sends.**
  Where a key or a chord carries an ASCII control code, the line names it:

      18:01:51.517 [decided]  C-h (BS 0x08) → remapped to Back (BS 0x08)

  That pair is the reason WinRemap exists: Ctrl+H and Backspace look alike and behave differently in a terminal. **Letters and digits never show a code** — WinRemap logs keys, not what you typed, and the log still never touches the disk.
  - A terminal sends DEL `0x7f` for the Backspace *key* and BS `0x08` for Ctrl+H, and an application may bind the two to different things. Remapping `C-h` to `Back` is what makes your terminal send `0x7f` for both. The log shows `BS 0x08` on both sides because that is the code **Windows itself** gives the Backspace key

- **Two views of the log.**
  The default is one line per key, saying what WinRemap decided. Tick **Every event** for the whole stream: every physical press and release, and everything WinRemap sent in reply. **The two halves of a remap happen at different moments** — the target is pressed when you press the key and released when you let go. Every line is recorded either way, so ticking the box explains the keys you **already** pressed.

- **It is readable now.**
  Every line carries the time to the millisecond and a tag saying which stream it belongs to (`[input]`, `[decided]`, `[injected]`, `[action]`, `[window]`). A stamp equal to the one above it is left blank, so everything WinRemap did in reply to one press reads as a group. The report for the application in front is one line naming the `application` value to write and the keymaps it reaches. The window opens larger.

- **Keys are named.**
  A key the config cannot name yet reads as `Num1 (0x61)` rather than `0x61`, and punctuation shows **the character your own keyboard layout prints on it** (`0xC0` is `@` on a Japanese layout, a backquote on a US one).

- **The log window says which config file is loaded**, instead of only mentioning it on a reload.

- **The settings and log windows are readable by screen readers** (UI Automation).

### Changed

- **`--debug` now decides whether a terminal gets anything.** It used to print whenever one was attached, so starting WinRemap from a shell produced a running commentary nobody asked for. Without the flag it is now **as quiet as a launch from Explorer**; with it, the terminal gets **exactly what the log window shows**, stamps and tags included. Errors and `--help` are not log output and print as before.

### Install

- Installer: `winremap-setup.exe` (per-user, no admin rights needed)
- Portable: `winremap.exe` (one file; config at `%APPDATA%\winremap\config.toml`)

If `SmartScreen` warns you ("Windows protected your PC"), it is because the binary is unsigned. **More info → Run anyway**. Verifying your download is recommended:

### Verify your download

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # compare with SHA256SUMS
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

Binaries distributed anywhere other than GitHub Releases are unofficial.

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v0.5.0/CHANGELOG.md
