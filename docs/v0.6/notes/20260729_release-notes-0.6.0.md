## WinRemap v0.6.0 — Microsoft Store から入れられるようになります

機能ではなく**配布経路**の版です。「Windows によって PC が保護されました」を消すために、Store 版を用意しました。

### 新機能

- **Microsoft Store 版**
  Microsoft がパッケージに署名するため、**SmartScreen の警告が出ません**。更新も自動で届きます。GitHub からの直接ダウンロードは今までどおりで、そちらは引き続き未署名です（検証手順は下記）。
  - 中身は同じソース・同じタグから作った同じビルドです。**どちらを選ぶかは信頼の問題ではなく**、インストールと更新の好みの問題です
  - ストアページは**認定を通り次第**公開されます: https://apps.microsoft.com/detail/9N6TQDXRX5WV
  - 2 つの経路の違い（更新・設定ファイルの場所・ポータブル利用の可否）は[インストールガイド](https://daikisuganuma.github.io/winremap/ja/install.html)にまとめました

- **設定ファイルが無ければ、初回起動時に作ります**
  これまでは設定ファイルが無いと起動を拒否していました。インストーラーがファイルを置いていたので気づきにくかったのですが、**ポータブル版を初めて動かすと必ず失敗する**状態でした。今は最小サンプル（メモ帳の `Ctrl+H`）を作り、どこに作ったかをログに出します。
  - **既にある設定を上書きすることはありません**
  - `--config` でパスを明示した場合は従来どおり、そのファイルが無ければエラーです。打ち間違いを黙って通すと、何もリマップしない WinRemap ができあがるためです

### 修正

- **起動元のターミナルを閉じても、WinRemap が道連れで終了しなくなりました**
  `--debug` の出力先を確保するためにそのターミナルのコンソールへ接続していましたが、Windows はコンソールのウィンドウが閉じられると**そこに接続している全プロセスを終了させます**。常駐に入った時点でコンソールを手放すようにしました。`--debug` を付けた場合はこれまでどおり端末に出力し、端末と運命を共にします（それがこのオプションの用途です）。

- **Store 版が、実在する設定ファイルのパスを表示します**
  パッケージ版は `%APPDATA%` への書き込みが非公開の場所へ振り替えられます。アプリ自身には見えませんが、**パスを他のプログラムに渡した瞬間に露見します** — 「テキストエディタで開く」「フォルダーを開く」が、エクスプローラーにもエディターにも存在しない場所を指していました。起動時に 1 回だけ解決するようにしたので、アドレスバーの表示・ファイル監視・外部起動がすべて同じ 1 つの場所を指します。
  - インストーラー版から乗り換えた場合は、**それまでの設定ファイルがそのまま使われます**（既定の設定で置き換えられません）

- ヘルプサイトが `examples/suganuma.toml` にリンクしていました。0.5.0 で `personal-ja.toml` に改名したファイルで、日英どちらのページでも 404 になっていました。

### インストール

- **Microsoft Store**: 認定通過後に公開されます（警告なし・自動更新）
- インストーラー: `winremap-setup.exe`（管理者権限不要・ユーザー単位）
- ポータブル: `winremap.exe`（1 ファイル。設定は `%APPDATA%\winremap\config.toml`）

`SmartScreen`（「Windows によって PC が保護されました」）は**GitHub から落としたファイル**に出ます。署名なしのためです。**詳細情報 → 実行**で起動できます。下記の検証を推奨します。Store 版ではこの警告自体が出ません。

### ダウンロードの検証

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # SHA256SUMS と照合
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

公式の配布経路は **Microsoft Store と GitHub Releases の 2 つ**です。それ以外で配布されているバイナリは非公式です。

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v0.6.0/CHANGELOG.md

---

## WinRemap v0.6.0 — Now on the Microsoft Store

A release about **how WinRemap reaches you**, not about what it does. The point of it is to make "Windows protected your PC" go away.

### New

- **A Microsoft Store version.**
  Microsoft signs the package, so **there is no SmartScreen warning**, and updates arrive on their own. Downloading from GitHub works exactly as before, and those binaries remain unsigned (verification below).
  - It is the same build from the same source at the same tag. **Choosing between them is not a question of trust** — it is a question of how you like to install and update software
  - The Store listing goes live **when certification completes**: https://apps.microsoft.com/detail/9N6TQDXRX5WV
  - What actually differs between the two — updates, where your config file lives, whether portable use is possible — is laid out in the [install guide](https://daikisuganuma.github.io/winremap/install.html)

- **First run creates a config if you have none.**
  WinRemap used to refuse to start without one. That was easy to miss because the installer seeded the file for you — but it meant **the portable exe failed the first time you ran it, every time**. It now writes the minimal example (the Notepad `Ctrl+H` fix) and says where it put it.
  - **An existing config is never overwritten**
  - A path given with `--config` still has to exist. A typo there should say so, rather than quietly producing a WinRemap that remaps nothing

### Fixed

- **Closing the terminal you started WinRemap from no longer closes WinRemap.**
  It attached to that terminal's console so `--debug` output would have somewhere to go, and Windows kills **every process attached to a console** when its window closes. A normal launch now hands the console back as soon as remapping is live. With `--debug` it still streams to the terminal and still ends with it, which is what that flag is for.

- **The Store build reports a config path that exists.**
  A packaged app has its `%APPDATA%` writes redirected somewhere private. WinRemap itself cannot see that — but **the moment it hands the path to another program, it shows**: "open in text editor" and "open folder" pointed at a location that was not there for Explorer or your editor. The path is now resolved once at startup, so the address bar, the file watch and both links agree on one location.
  - Switching over from the installed version **keeps using the config you already had**, rather than replacing it with a fresh default

- The help site linked `examples/suganuma.toml`, renamed to `personal-ja.toml` back in 0.5.0 — the link 404'd on both language versions.

### Install

- **Microsoft Store**: live once certification completes (no warning, automatic updates)
- Installer: `winremap-setup.exe` (per-user, no admin rights needed)
- Portable: `winremap.exe` (one file; config at `%APPDATA%\winremap\config.toml`)

`SmartScreen` ("Windows protected your PC") applies to **files downloaded from GitHub**, because those are unsigned. **More info → Run anyway**, and verifying your download is recommended. The Store version never shows it.

### Verify your download

    (Get-FileHash .\winremap-setup.exe -Algorithm SHA256).Hash.ToLower()   # compare with SHA256SUMS
    gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap

There are **two** official channels: the Microsoft Store and GitHub Releases. Binaries distributed anywhere else are unofficial.

**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/v0.6.0/CHANGELOG.md
