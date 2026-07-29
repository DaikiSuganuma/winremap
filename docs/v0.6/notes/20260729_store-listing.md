# Microsoft Store 掲載情報の草案（v0.6.0）

- 作成日: 2026-07-29
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- 対象: Store ID `9N6TQDXRX5WV`（`SUGANUMADaiki.WinRemap`）
- 関連: [ADR 0060](../decisions/0060-msix-package.md)（MSIX 構成）、[ADR 0061](../decisions/0061-packaged-config-path.md)

Partner Center の「ストアの掲載情報」に投入する文面。**日本語と英語の 2 言語**で登録する（パッケージが両方の UI を持つため）。

> **文字数上限について**: 公式ドキュメントで確実に確認できたのは「機能は最大 20 項目・各 200 文字」「説明とスクリーンショット 1 枚が必須」の 2 点のみ。他の欄の上限は Partner Center の入力欄がカウンターを表示するので、投入時に確認する。本草案は短めに寄せてある。

---

## 1. 日本語

### 短い説明

> アプリごとにキーを割り当て直せる、Windows 用のキーリマッパー。Emacs 風の記法で設定を書き、設定ウィンドウでその場で編集できます。キー入力の記録も通信も一切行いません。

### 説明

> **WinRemap は、前面にあるアプリごとにキーの割り当てを変えられるキーリマッパーです。**
>
> ターミナルでは `Ctrl+H` を Backspace にしたい。でもブラウザーでは元のままがいい。WinRemap はそういう「アプリによって変えたい」を設定ファイルに 1 行書くだけで実現します。
>
> **仕組みはシンプルです。** WinRemap が行うのはキー入力の置き換えだけで、アプリの機能を直接呼び出すことはありません。物理キーの入力を抑止し、代わりのキーを送出します。アプリは、あなたがそう打ったものとして自分の流儀で解釈します。`Alt+A` を `Ctrl+A` に割り当てれば、そのアプリが Ctrl+A に対して行うこと（多くは「すべて選択」）が起きます。
>
> **設定は TOML ファイル 1 つ。** `C-h`、`A-f`、`Back` といった Emacs 風の記法で書けるので、Keyhac や fakeymacs をお使いだった方はそのままの感覚で移行できます。設定ウィンドウを開けば、いま効いている設定を一覧で確認でき、その場で編集して保存できます。あなたが書いたコメントも、空行も、並び順も、触っていない箇所はそのまま残ります。
>
> **キー入力は記録しません。** WinRemap は低レベルキーボードフックを使うため、押したキーはすべてこのプログラムを通過します。だからこそ明言します — 入力内容をディスクに書くことも、メモリに残すことも、どこかへ送ることもありません。ネットワーク通信を行うコードがそもそも含まれていません。ソースコードは GitHub で公開しており、ご自身で確認いただけます。
>
> 変換されるのは、あなたが設定ファイルに書いたルールだけです。
>
> **主な機能**
>
> ・アプリ別のリマップ（`notepad.exe` だけ、あるいは `*` で全体に。除外リストも指定可）
> ・2 ストロークのキーシーケンス（`A-x h` のような Emacs 風プレフィックス）
> ・マクロ出力（1 つのキーに複数のキー操作を割り当て）
> ・マクロ記憶（キーを押して記録開始、普段どおり操作、もう一度押して終了。再生キーで繰り返す。記録はメモリ上だけで、終了すると消えます）
> ・タスクトレイ常駐（有効/無効の切り替え、設定の再読み込み、終了）
> ・設定ウィンドウ（現在の設定の確認とその場での編集。保存前に検証します）
> ・ログウィンドウ（どのキーをどう判定したかを 1 行ずつ確認。ファイルには書きません）
> ・IME 状態インジケーター（任意。IME がオンになった瞬間に半透明の「あ」を表示。表示のみで、IME の切り替えは行いません）
> ・日本語・英語の UI（システムの言語から自動選択）
>
> **既知の制限**: 管理者権限で実行されているウィンドウには、Windows の仕様（UIPI）によりリマップが効きません。
>
> ヘルプ: https://daikisuganuma.github.io/winremap/ja/
> ソースコード: https://github.com/DaikiSuganuma/winremap

### 機能（Partner Center の「機能」欄・箇条書き）

1. アプリごとに違うキー割り当て — 前面のアプリを見て、そのアプリ用のルールを適用します
2. Emacs 風のキー記法（`C-h`、`A-f`、`W-Left`）で書ける TOML 設定ファイル
3. 設定ウィンドウで、いま効いている設定の確認とその場での編集
4. 2 ストロークのキーシーケンスとマクロ出力
5. 操作を記録して 1 キーで再生するマクロ記憶（メモリ上のみ・保存しません）
6. キー入力の記録なし・ネットワーク通信なし。ソースコードは GitHub で公開
7. タスクトレイ常駐。起動時にコンソールが一瞬出ることもありません
8. 日本語・英語の UI

### 検索キーワード

`キーリマップ` / `キー割り当て` / `キーバインド` / `Emacs` / `Keyhac` / `CapsLock` / `keyboard`

### このバージョンの新機能

> WinRemap の Microsoft Store 版です。初回起動時に既定の設定ファイルを作るようになり、インストール直後からすぐ使い始められます。

---

## 2. English

### Short description

> A per-application key remapper for Windows. Write your rules in Emacs-style
> notation and edit them in place from the settings window. It logs no
> keystrokes and makes no network connections.

### Description

> **WinRemap changes what your keys do, differently in each application.**
>
> You want `Ctrl+H` to be Backspace in your terminal, but left alone in the
> browser. WinRemap makes that one line in a configuration file.
>
> **What it does is deliberately simple.** WinRemap only replaces keystrokes —
> it never invokes application functions directly. It suppresses the physical
> key event and injects the replacement. The application receives the injected
> keys as if you had typed them and applies its own native meaning: remap
> `Alt+A` to `Ctrl+A` and the app runs whatever it does for Ctrl+A, usually
> Select All.
>
> **One TOML file holds your rules,** written in Emacs-style notation (`C-h`,
> `A-f`, `Back`) that will look familiar if you have used Keyhac or fakeymacs.
> Open the settings window to see the configuration that is in effect right
> now — every keymap, its target apps, its rules, your own comments beside
> them — and edit it in place. Everything you did not touch comes back
> unchanged: comments, blank lines, ordering, spellings.
>
> **It does not log your keystrokes.** WinRemap installs a low-level keyboard
> hook, which means every key you press passes through it. So it should be
> said plainly: what you type is never written to disk, never retained in
> memory, and never sent anywhere. There is no networking code in the program
> at all. The source is on GitHub and you are welcome to check.
>
> Only the rules you wrote are ever applied.
>
> **Features**
>
> · Per-application rules (just `notepad.exe`, or `*` for everything with an
> optional exclude list)
> · Two-stroke sequences (`A-x h`, Emacs-style prefix keys)
> · Macro output — one key sends a series of keystrokes
> · Macro recording: press to start, work as usual, press to stop, and a
> replay key repeats it. Held in memory only; gone when WinRemap exits
> · Task tray resident: enable/disable, reload the config, quit
> · Settings window with validation before it saves
> · Log window showing what WinRemap decided, key by key. Nothing is written
> to disk
> · Optional IME status indicator — a translucent panel the moment the IME
> turns on. Display only; WinRemap never switches the IME
> · Japanese and English UI, following the system language
>
> **Known limitation**: remapping does not reach windows running elevated, by
> Windows design (UIPI).
>
> Help: https://daikisuganuma.github.io/winremap/
> Source: https://github.com/DaikiSuganuma/winremap

### Product features

1. Different key mappings per application — rules follow the app in front
2. TOML configuration in Emacs-style notation (`C-h`, `A-f`, `W-Left`)
3. Settings window: see the configuration in effect and edit it in place
4. Two-stroke key sequences and macro output
5. Macro recording — record what you did, replay it with one key (memory only)
6. No keystroke logging, no network access. Source published on GitHub
7. Task tray resident; never flashes a console window on launch
8. Japanese and English UI

### Search terms

`key remapper` / `keyboard` / `remap` / `keybindings` / `Emacs` / `Keyhac` / `CapsLock`

### What's new in this version

> WinRemap on the Microsoft Store. First run now creates a starter
> configuration, so it works the moment you install it.

---

## 3. `runFullTrust` の理由説明（提出フォーム用・英語）

制限付き機能の使用理由を求められた場合に貼る文面。**技術的な必然性**と**濫用しないこと**の 2 点を答える。

> WinRemap is a keyboard remapper. It works by installing a Win32 low-level
> keyboard hook (`SetWindowsHookEx` with `WH_KEYBOARD_LL`) to observe physical
> key events before they reach the foreground application, and by injecting
> the replacement keystrokes with `SendInput`.
>
> Both are classic Win32 APIs that require an ordinary full-trust, medium-IL
> desktop process. They are not available inside an app container, and the
> WinRT input APIs are not a substitute: they can inject input but cannot
> observe and suppress a physical key event, which is the entire function of
> this app.
>
> WinRemap uses this capability for nothing else. It remaps only the keys the
> user has written into their own configuration file, which they can view and
> edit in the app's settings window. It does not record, store or transmit
> keystrokes, and it contains no networking code of any kind — no telemetry,
> no auto-update.
>
> Source code: https://github.com/DaikiSuganuma/winremap
> Privacy policy: https://daikisuganuma.github.io/winremap/privacy.html

### ストアポリシー 10.2（利用者の設定変更に同意を得る）への備え

審査で問われた場合の答えは掲載文にも含めてある。「変換されるのは、あなたが設定ファイルに書いたルールだけ」— 既定では何も変換せず、利用者が明示的に書いたルールのみを適用する。既定の設定ファイルも Notepad の `Ctrl+H` 1 件のみで、システム全体には影響しない。

---

## 4. スクリーンショット計画

必須は 1 枚、最大 10 枚。推奨解像度は 1366×768 以上。

| # | 内容 | 意図 |
|---|---|---|
| 1 | 設定ウィンドウ（キーマップ一覧が見える状態） | 主画面。何ができるアプリかが 1 枚で伝わる |
| 2 | 設定ウィンドウの編集モード（ルールを編集中） | GUI で編集できることを示す |
| 3 | ログウィンドウ（判定行が並んでいる状態） | 「何が起きているか見える」ことの証拠 |
| 4 | トレイメニューを開いたところ | 常駐アプリであることを示す |
| 5 | IME インジケーターが出ているところ | 任意機能の紹介 |
| 6 | 設定ファイル（TOML）の例 | 設定の実体を見せる |

**撮影は個人情報が写り込まない環境で行う。** デスクトップ全体を撮ると、開いているウィンドウやファイル名が入る。ウィンドウ単体を `PrintWindow` で取得し、無地の背景に合成する方式にすれば、画面上の他の情報は一切含まれない。UI テスト用の VM（`tests/ui/run-vm-ui-test.ps1`）を使えば、そもそも素のデスクトップから撮れる。

---

## 5. 未確定事項

- 各欄の文字数上限（Partner Center の入力欄で確認する）
- **プライバシーポリシー URL は `main` にリリースするまで 404**。GitHub Pages は `main` からのみ発行される（`.github/workflows/pages.yml`）ため、**Store 提出は v0.6.0 のリリース後**になる
- 年齢レーティング（アンケート形式。オーナーが Partner Center で回答）
- 掲載言語を日英 2 つとするか、英語のみで出すか（2 言語を推奨。パッケージが両方の UI を持つため）
