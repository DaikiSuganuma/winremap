# v1.0.1 リリースノート（ドラフト本文）

- 作成日: 2026-09-14
- 作成: Claude Code（AI モデル: claude-opus-5）／公開: オーナー
- 用途: [リリース手順 §2 手順 6](../../03_release-operations.md) — `gh release edit v1.0.1 --notes-file <このファイルの本文部分>`。オーナーの仕事が Publish を押すことだけになるように、本文をそのまま入れられる形で置く
- 体裁は [v1.0.0 のノート](../../v1.0/notes/20260816_release-notes-1.0.0.md)に合わせた（日本語 → `---` → 英語）
- 根拠: [ADR 0080](../decisions/0080-tray-icon-asks-for-the-small-metric.md)・[ADR 0081](../decisions/0081-icon-must-not-depend-on-its-background.md)・[ADR 0082](../decisions/0082-small-master-for-small-sizes.md)、[CHANGELOG 1.0.1](../../../CHANGELOG.md)
- 公式参照: [Notifications and the notification area（Win32）](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area)

---

## WinRemap v1.0.1 — トレイのアイコンと表示名を直しました

1.0.0 のリリース後に気づいた、タスクトレイまわりの 2 件を直した修正だけの版です。新機能はありません。

### 修正

- **タスクトレイのアイコンが、青い塗りつぶしではなくキーボードの絵で出るようになりました**
  設定 > 個人用設定 > タスクバー > **その他のシステム トレイ アイコン**の一覧で、WinRemap の行のアイコンが**青い四角**に見えていました。
  - キーの白い部分は、本体を「くり抜いて」表していました。この一覧はアイコンをアクセント カラーの台の上に描くので、青い本体とくり抜いた穴の向こうの台が同じ色になり、キーが消えていました
  - キーを白で「塗る」形に描き直しました。明るいタスクバー・暗いタスクバー・その一覧のどれでも同じように見えます
  - **設定アプリの一覧は、行が最初に作られたときのアイコンを保存して使い続けます。** Microsoft Store 版は更新するとインストール先が変わるので、新しい行ができて新しいアイコンになります。**インストーラー版とポータブル版はインストール先が変わらないので、一覧には古い絵が残ります。** タスクトレイのアイコンそのものは、どの版でも新しい絵になります

- **タスクトレイのアイコンがぼやけなくなりました**
  アイコンを大きいサイズで Windows に渡し、トレイの大きさへ縮めさせていました。トレイが実際に描くサイズで渡すようにしたので、縮小によるにじみが出ません。
  - 画面の拡大率が 125% と 175% のときに使われる 20・28 ピクセルの絵が入っていなかったので、足しました
  - 絵も描き直し、16・32 ピクセルでは辺がちょうど画素の境目に来るようにしました

- **タスク マネージャーなどで「WinRemap」と表示されるようになりました**
  実行ファイルに説明・製品名・発行元が入っていなかったので、Windows は小文字の `winremap` と表示し、発行元も空欄でした。説明と製品名を `WinRemap`、発行元を `SUGANUMA Daiki` にしました。

### インストール

- **Microsoft Store**: https://apps.microsoft.com/detail/9N6TQDXRX5WV （警告なし・自動更新）
- **winget**: `winget install DaikiSuganuma.WinRemap`（1.0.1 のマニフェストは公開後に提出します。それまでは 1.0.0 が入ります）
- インストーラー: `winremap-setup.exe`（管理者権限不要・ユーザー単位）
- ポータブル: `winremap.exe`（1 ファイル。設定は `%APPDATA%\winremap\config.toml`）

`SmartScreen`（「Windows によって PC が保護されました」）は**GitHub から落としたファイル**に出ます。署名なしのためです。**詳細情報 → 実行**で起動できます。下記の検証を推奨します。Store 版ではこの警告自体が出ません。

### ダウンロードの検証

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # SHA256SUMS と照合
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

公式の配布経路は **Microsoft Store・winget・GitHub Releases の 3 つ**です。それ以外で配布されているバイナリは非公式です。

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v1.0.1/CHANGELOG.md

---

## WinRemap v1.0.1 — the tray icon and the app's name

A fix-only release for two things about the tray noticed after 1.0.0. Nothing new.

### Fixed

- **The tray icon is drawn as a keyboard, not a blue block**
  In Settings > Personalization > Taskbar > **Other system tray icons**, WinRemap's row showed a **plain blue square**.
  - The white keys were cut out of the body. That list draws every icon on a plate in your accent colour, so the blue body and the plate showing through the holes were the same colour, and the keys disappeared
  - The keys are now painted white on the body. The icon reads the same on a light taskbar, a dark one, and that list
  - **The Settings list keeps the icon it saved when the row was first created.** The Microsoft Store version installs each release in a new location, so updating creates a new row with the new icon. **The installer and the portable exe stay where they are, so the list keeps the old picture for them.** The tray icon itself is the new one in every version

- **The tray icon is no longer blurred**
  The icon was handed to Windows at the large size and shrunk to fit the tray. It is now handed over at the size the tray actually draws, so nothing is rescaled.
  - The 20 and 28 pixel images, used at 125% and 175% display scaling, were missing and have been added
  - The artwork was redrawn so that every edge falls on a whole pixel at 16 and 32 pixels

- **Task Manager and other places now say "WinRemap"**
  The executable carried no description, product name or publisher, so Windows showed a lower-case `winremap` with no publisher. The description and product name are now `WinRemap`, and the publisher is `SUGANUMA Daiki`.

### Install

- **Microsoft Store**: https://apps.microsoft.com/detail/9N6TQDXRX5WV (no warning, auto-updates)
- **winget**: `winget install DaikiSuganuma.WinRemap` (the 1.0.1 manifest is submitted after this release is published; until then you get 1.0.0)
- Installer: `winremap-setup.exe` (per-user, no admin rights)
- Portable: `winremap.exe` (single file; config at `%APPDATA%\winremap\config.toml`)

SmartScreen ("Windows protected your PC") appears for **files downloaded from GitHub**, because they are unsigned. Choose **More info → Run**. Verifying your download is recommended; the Store build does not raise the warning at all.

### Verify your download

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # compare with SHA256SUMS
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

The official channels are **Microsoft Store, winget and GitHub Releases**. Binaries distributed anywhere else are unofficial.

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v1.0.1/CHANGELOG.md
