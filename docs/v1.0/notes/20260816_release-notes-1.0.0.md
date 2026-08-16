# v1.0.0 リリースノート（ドラフト本文）

- 作成日: 2026-08-16
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／公開: オーナー
- 用途: [リリース手順 §2 手順 6](../../03_release-operations.md) — `gh release edit v1.0.0 --notes-file <このファイルの本文部分>`。**オーナーの仕事が Publish を押すことだけになる**ように、本文をそのまま入れられる形で置く
- 体裁は [v0.7.0 のノート](../../v0.7/notes/20260802_release-notes-0.7.0.md)・v0.9.0 の本文に合わせた（日本語 → `---` → 英語）

---

## WinRemap v1.0.0 — 使っていて気づいたことを、直しました

1.0 は何かを凍結する宣言ではありません。設定ファイルの書式や CLI に新しい互換性の約束を足すものでもありません（これまでどおり気は配ります）。**いまの機能で、作者自身が使い続けて満足している**、という表明です。

**この版の 4 つの変更は、すべて「使っていて気づいたこと」から始まりました。**

### 新機能

- **前回選んだ設定ファイルで起動します**
  設定ウィンドウのアドレスバーでは 0.4.0 から設定フォルダーの `*.toml` を切り替えられましたが、**その選択はそのとき動いている WinRemap にしか効きませんでした** — 次に起動すると黙って `config.toml` に戻り、画面には何も出ません。毎朝サインインのたびに選び直すことになります。
  - 選んだファイルは `%APPDATA%\winremap\last-config.txt`（1 行のテキスト）に記録され、次の起動がそれを開きます。**このファイルを消せば既定に戻ります**
  - **記録されるのはウィンドウで選んだときだけです。** `--config` はその 1 回の起動への指示なので、記録も参照もしません
  - 記録したファイルの名前を変えたり消したりしていた場合は、**既定の設定で起動し、探しに行った名前をログに書きます**

- **タスクトレイのメニューが、いま効いている設定ファイルの名前を出します**
  バージョンの下に 1 行増えました。設定ファイルが 1 つとは限らなくなった以上、「いま何で動いているのか」はトレイが答えるべき質問です。ウィンドウで切り替えれば、次に開いたメニューの表示も変わります。

### 修正

- **拡大表示の画面で、I ビームに IME の色が付くようになりました**
  画面の拡大率が 125% 以上のとき、**矢印には色が付くのに、文字入力の I ビームには付きません**でした。0.9.0 で報告を受けて調べたところ、**3 つの版にわたって追いかけていた症状の原因**でした。
  - WinRemap は画面ごとの拡大率に対応したプログラムです。そういうプログラムが Windows に I ビームを求めると、拡大表示では**中身が何も描かれていないカーソル**が返ってきます。素の I ビームは画面を反転させて描くカーソルで、その形は拡大にともなう変換で消えてしまうためです。矢印はふつうのカラーカーソルなので影響を受けず、**着色がちょうど半分だけ欠ける**形になっていました
  - 空が返ってきたときだけ、**拡大率を知らない文脈で読み直す**ようにしました。矢印はこれまでどおりこの画面の大きさのまま使われます
  - **0.8.0 の「色付きの I ビームがまったく描かれない」も、同じ原因の別の見え方でした。** 拡大表示の環境では、カーソルを元に戻すための複製も採れていませんでした（そちらも直っています）
  - `change_cursor_color` を使っていない場合、実害はありませんでした

- **設定フォルダーのパスが長くても、設定ウィンドウのボタンが窓から出なくなりました**
  アドレスバーはフォルダーのパスをそのまま出していたため、深い場所に設定があると（**Microsoft Store 版のフォルダーがそうです**）、ファイルのプルダウンと「編集」ボタンが窓の右端の外へ押し出され、**押せなくなっていました**。
  - エクスプローラーと同じように、先頭と末尾を残して途中を `…` にします。**窓の幅を変えると、それに合わせて伸び縮みします**
  - 省略されているときは、パスにマウスを乗せると全体が読めます

### インストール

- **Microsoft Store**: https://apps.microsoft.com/detail/9N6TQDXRX5WV （警告なし・自動更新）
- **winget**: `winget install DaikiSuganuma.WinRemap`（1.0.0 のマニフェストは公開後に提出します。それまでは 0.9.0 が入ります）
- インストーラー: `winremap-setup.exe`（管理者権限不要・ユーザー単位）
- ポータブル: `winremap.exe`（1 ファイル。設定は `%APPDATA%\winremap\config.toml`）

`SmartScreen`（「Windows によって PC が保護されました」）は**GitHub から落としたファイル**に出ます。署名なしのためです。**詳細情報 → 実行**で起動できます。下記の検証を推奨します。Store 版ではこの警告自体が出ません。

### ダウンロードの検証

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # SHA256SUMS と照合
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

公式の配布経路は **Microsoft Store・winget・GitHub Releases の 3 つ**です。それ以外で配布されているバイナリは非公式です。

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v1.0.0/CHANGELOG.md

---

## WinRemap v1.0.0 — the things I noticed while using it

The 1.0 is not a freeze, and it adds no new compatibility promise about the config format or the CLI (they get the same care they always have). It says the feature set is one its author is happy to keep using.

**All four changes in this release started as something noticed in daily use.**

### New

- **WinRemap opens the config file you chose last time**
  The settings window has been able to switch between the `*.toml` files in your config folder since 0.4.0, but the choice only lasted as long as that run: every start went back to `config.toml`, with nothing on screen to say why the keys had changed. If your real setup lives in another file, you were choosing it again after every sign-in.
  - The file you pick is recorded in `%APPDATA%\winremap\last-config.txt` (one line of text) and the next start opens it. **Delete that file and the default applies again**
  - **Only a choice made in the window is remembered.** `--config` is an instruction for one run and neither reads nor writes the memory
  - If the remembered file has been renamed or deleted, WinRemap starts on the default and says so, naming the file it went looking for

- **The tray menu names the config file in force**
  A caption line under the version. With more than one config file possible, "which one am I running?" is a question the tray should answer. Switch files in the settings window and the next menu you open says so.

### Fixed

- **The I-beam takes the IME tint on a scaled display**
  At 125% scaling or more, the arrow was tinted while the text I-beam stayed plain. Reported on 0.9.0, and the cause turned out to be **the same one behind a symptom chased across three releases**.
  - WinRemap is per-monitor DPI aware, and a program like that asking Windows for the I-beam on a scaled display is handed a cursor with **nothing drawn in it**: the stock I-beam is drawn by inverting the screen, and that does not survive the conversion scaling brings. The arrow is an ordinary colour cursor and was unaffected — which is why exactly half the tint went missing
  - A cursor that comes back empty is now read again from a context that knows nothing about scaling. The arrow keeps being used at this display's size
  - **0.8.0's "the tinted I-beam is not drawn at all" was the same cause wearing a different face.** On a scaled display the pristine copy used to restore your cursor could not be taken either; that is fixed too
  - Nothing was affected for anyone who leaves `change_cursor_color` off

- **A long config path no longer pushes the settings window's buttons off the screen**
  The address bar showed the folder path in full, so with the config in a deep folder — **a Microsoft Store install's own folder is one** — the file dropdown and the Edit button were carried past the right edge of the window, where they could not be clicked at all.
  - The path is now shortened Explorer style, keeping both ends and eliding the middle, and it **grows and shrinks as you resize the window**
  - Hover it to read the whole thing

### Install

- **Microsoft Store**: https://apps.microsoft.com/detail/9N6TQDXRX5WV (no warning, auto-updates)
- **winget**: `winget install DaikiSuganuma.WinRemap` (the 1.0.0 manifest is submitted after this release is published; until then you get 0.9.0)
- Installer: `winremap-setup.exe` (per-user, no admin rights)
- Portable: `winremap.exe` (single file; config at `%APPDATA%\winremap\config.toml`)

SmartScreen ("Windows protected your PC") appears for **files downloaded from GitHub**, because they are unsigned. Choose **More info → Run**. Verifying your download is recommended; the Store build does not raise the warning at all.

### Verify your download

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # compare with SHA256SUMS
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

The official channels are **Microsoft Store, winget and GitHub Releases**. Binaries distributed anywhere else are unofficial.

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v1.0.0/CHANGELOG.md
