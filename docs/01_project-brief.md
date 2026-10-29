# WinRemap — Project Brief（開発経緯と設計判断 / AIエージェント向け資料）

> 本ドキュメントは WinRemap の開発背景・設計判断・技術的制約をまとめた一次資料である。
> AI エージェント（Claude Code 等）はコーディング規約 `AGENTS.md` と併せて、実装前に必ず全文を読むこと。

- **プロジェクト名**: WinRemap（表示名。crate 名・exe 名などの技術識別子は小文字 `winremap`。[ADR 0025](v0.1/decisions/0025-display-name-winremap.md)）
- **一行説明**: A per-application key remapper for Windows, written in Rust — inspired by xremap (Linux) and Keyhac
- **ライセンス**: MIT
- **主開発手法**: AI エージェント（Claude Code）による開発。人間のオーナー（菅沼）がレビュー・受け入れ判断を行う

---

## 1. 開発経緯（なぜこのプロジェクトが生まれたか）

この経緯はプロジェクトの設計判断の根拠になっているため、要約せず理解すること。

### 1.1 発端: Claude Code の Ctrl+H 問題

Claude Code（CLI）のプロンプト入力中に Ctrl+H を押すと、1文字ではなく単語単位で削除される問題があった。原因は以下のとおり。

- Backspace キー → ターミナルが `0x7f`（DEL）を送る → アプリ側で「1文字削除」
- Ctrl+H → ターミナルが `0x08`（BS）を送る → Claude Code の入力処理が「単語削除」に割り当てている

Windows Terminal では `sendInput` アクションで Ctrl+H に `\u007f` を送らせることで解決した（ブログ記事: https://blog.dksg.jp/2026/06/claude-code-ctrlh-1.html ）。しかし PHPStorm（JetBrains IDE）の内蔵ターミナルには VS Code の `sendSequence` に相当する機能が存在せず、IDE 側の設定では解決できなかった（JetBrains 公式フォーラムで機能なしと回答、関連 feature request: IDEA-241798）。

### 1.2 既存環境: Keyhac + fakeymacs

Windows 環境では長年、Keyhac（craftware 氏作、Python スクリプトで設定を書くキーカスタマイズツール、MIT License）に、fakeymacs（smzht 氏作、Keyhac 上で Emacs 風キーバインドを実現する設定集、MIT License）を載せて使ってきた。個人用フォーク: https://github.com/DaikiSuganuma/fakeymacs

PHPStorm の Ctrl+H 問題は、fakeymacs の `config_personal.py` で `fc.not_emacs_target` に `phpstorm64.exe` が登録されており Emacs キーバインド変換（C-h → Backspace）が働いていなかったことが原因と判明。Keyhac 側でアプリ別キーマップを追加すれば解決できることが分かった。

### 1.3 Rust による作り直しの決断

Keyhac は便利だが機能が多く設定ファイルも複雑なため、必要な機能だけを Rust で1から作り直すことにした。動機は次のとおり。

1. **安定性**: Keyhac は WH_KEYBOARD_LL フックの処理を Python で行うため、GC や処理遅延でフックのタイムアウト（Windows によるフック切り離し）が起こり、文字の二重入力等の原因になる。fakeymacs の README では `LowLevelHooksTimeout` レジストリを 1000ms に延長する回避策が「Windows 11 ではほぼ必須」とされている。Rust ならフック内処理がマイクロ秒オーダーで完結し GC も無いため、**この問題が原理的に消える**。レジストリ回避策も不要になる
2. **最小構成**: 実際に使っている機能は少数。設定を宣言的なファイル（TOML）に絞りシンプルにする
3. **配布性**: 単一バイナリ、依存なし、起動が速い

なお、体感のタイピング速度（平均レイテンシ）は Python でも数 ms であり劇的には変わらない。改善されるのは最悪値（テールレイテンシ）と安定性である。この認識を README 等で誇張しないこと。

### 1.4 名前の決定

候補として kaeru / sabimacs / emacskeyd 等を検討した結果、**WinRemap** に決定した。決め手は、Linux の Rust 製キーリマッパー **xremap**（アプリ別リマップ・Emacs 風設定・マークモード対応）の Windows 版に相当するというポジショニングが名前だけで伝わること。WinMerge / WinSCP 等の「Win + 機能名」という Windows ツールの命名慣習にも沿う。

2026年7月時点の調査で GitHub / crates.io に同名の競合プロジェクトは確認されていない。

2026-07-19 追記: 表示名は WinMerge / WinSCP と同じキャメルケース「WinRemap」に統一した（技術識別子は小文字のまま。ADR 0025）。

### 1.5 設計思想の決定

xremap と同じく「**本体は汎用のアプリ別キーリマッパー、Emacs 風キーバインドは同梱のサンプル設定**」という構造とする。Emacs 専用ツールにはしない。

---

## 2. ポジショニングと類似プロジェクト

| プロジェクト | プラットフォーム | 言語 | 特徴 | ライセンス |
|---|---|---|---|---|
| Keyhac | Windows | Python + C++ | 設定を Python で記述。本プロジェクトの発想の源流 | MIT |
| fakeymacs | Windows (Keyhac設定) | Python | Emacs 風キーバインド設定集。長年常用 | MIT |
| xremap | Linux (X11/Wayland) | Rust | アプリ別リマップ、キーシーケンス、Emacs 風設定＋マークモード。**最も近いアーキテクチャの参考** | MIT |
| kanata | クロスプラットフォーム | Rust | QMK 風レイヤー。デバイス寄りでアプリ別切替が弱い | **LGPL-3.0** |
| kmonad | クロスプラットフォーム | Haskell | kanata の源流 | MIT |
| XKeymacs | Windows | C++ | Windows 用 Emacs キーバインドの古典。開発停止 | — |
| 窓使いの憂鬱 / のどか / yamy | Windows | C++ | 日本製リマッパーの系譜 | — |
| universal-emacs-keybindings | Win/Mac | AutoHotkey / Karabiner | 全アプリに Emacs キーバインド適用 | — |

**「Windows × Rust × アプリ別リマップ」は空白地帯**であり、そこが WinRemap の立ち位置。

### ライセンス上の制約（厳守）

- **kanata（LGPL-3.0）のコードは一切移植・参照コピーしないこと**。設計アイデアの参考に留める
- Keyhac / fakeymacs / xremap / kmonad（いずれも MIT）からロジックを移植する場合は、`THIRD-PARTY-NOTICES.md` に元プロジェクトの著作権表示と MIT ライセンス文を記載すること
- ゼロから書いたコードのみで構成する場合でも、README の Acknowledgments に Keyhac（craftware 氏）、fakeymacs（smzht 氏）、xremap への謝辞を必ず記載する
- README 冒頭に「Keyhac の再実装・フォークではなく、影響を受けた独立プロジェクトである」「xremap とは無関係（Inspired by xremap; not affiliated）」を明記する

---

## 3. 要件

### 3.1 MVP（Minimum Viable Product: 実用最小限の製品 / v0.1）スコープ

1. WH_KEYBOARD_LL による低レベルキーボードフックでのキー捕捉
2. `SendInput` による代替キーの送出
3. **前面ウィンドウのプロセス名（exe 名）によるアプリ別キーマップ切り替え**（ワイルドカード指定 `*` でグローバル適用）
4. TOML 設定ファイル（`%APPDATA%\winremap\config.toml` を既定とし、`--config` で上書き可）
5. 設定のホットリロード（タスクトレイメニューまたはファイル監視）
6. タスクトレイ常駐（有効/無効トグル、設定、設定リロード、ログを表示、終了）
7. 同梱サンプル設定: `examples/minimal.toml`（Ctrl+H → Backspace のみ）、`examples/emacs.toml`（fakeymacs 相当の Emacs 風設定）

最初の実用ゴール（受け入れテスト）: **PHPStorm（phpstorm64.exe）でのみ Ctrl+H を Backspace として送り、Claude Code のプロンプトで1文字削除になること。他アプリには影響しないこと。**

### 3.2 v0.2 以降の候補（MVP に含めない）

- キーシーケンス対応（C-x C-c のような2ストローク）
- ワンショットモディファイア、押し分け（tap/hold）
- マークモード（xremap の Emacs 風設定にある C-Space 選択）
- ウィンドウタイトルによる切り替え（Windows Terminal のタブ検知に相当）
- 設定 GUI

### 3.3 Non-goals（作らないもの）

- **IME の制御・切り替え**（Emacs 日本語入力モード、無変換/変換キーでの IME 切替等）。IMM32/TSF の領域は複雑度が跳ね上がるため、OS 標準機能（Windows 11 の IME 設定）に任せる
  - 2026-07-19 改訂（オーナー指示）: IME 状態の**表示**（IME オン時のインジケーター表示）は例外として採用する（[ADR 0020](v0.1/decisions/0020-ime-indicator-scope.md)、設計: [08_ime-indicator-design.md](v0.1/05_ime-indicator-design.md)、計画: [09_ime-indicator-plan.md](v0.1/06_ime-indicator-plan.md)）。IME の**制御・切り替え**は引き続き Non-goal
- クリップボードリスト、ランチャー（Keyhac の付加機能は対象外）
- macOS / Linux 対応（Linux には xremap がある）
- **設定ファイル内のスクリプティング**（設定はあくまで宣言的な TOML。条件分岐・変数・任意コードの実行を設定に持ち込まない）
  - 2026-07-21 改訂（オーナー指示）: **実行時のキーボードマクロ記録・再生**（記憶キーで押した操作を覚え、再生キーで送出する）は例外として採用する（[ADR 0043](v0.3/decisions/0043-runtime-macro-recording.md)）。記録内容はメモリ上のみでプロセス終了時に破棄し、ディスクへは書かない（§5-6 のキーロガー化禁止との整合）。設定ファイルは宣言的なまま（記憶するのはキーであってスクリプトではない）で、**設定内スクリプティングは引き続き Non-goal**

---

## 4. アーキテクチャ

3層構造とし、**フック層は薄く、ロジック層は厚く**を原則とする。v1.1（[ADR 0086](v1.1/decisions/0086-hook-decisions-as-a-pure-engine.md)）で判断そのものもロジック層へ移り、フックは「訳して、決めてもらって、従う」だけになった。

```
[hook/]  WH_KEYBOARD_LL コールバック（unsafe 集約）
    │  KBDLLHOOKSTRUCT を KeyEvent に訳し、前面 exe とオプションを添える
    ▼
[engine + keymap + config]  純粋ロジック層（ライブラリ crate `winremap`、テスト可能）
    │  Engine::on_event → Decision（抑止するか、何を送るか、何を記録・ログするか）
    ▼
[sender.rs]  SendInput 送出（unsafe 集約）
       Decision の emit を 1 件送る / 元イベントは Decision.suppress で抑止
```

### モジュール責務

バイナリ（`src/main.rs` が宣言）とライブラリ（`src/lib.rs` が宣言）の 2 つの crate。ライブラリは `windows` クレートを参照しない（`tests/invariants.rs` が検査する）。

**ライブラリ `winremap`（純粋ロジック、ヘッドレス CI でテスト）**

| モジュール | 責務 |
|---|---|
| `keymap/` | キー記法のパースとマッチング（`parse` / `table` / `control` / `layout` スナップショット / `sides` 修飾キーの代数）。配列は `Layout` の値で受け取る（ADR 0063） |
| `config/` | TOML の読み込み・検証・コンパイル（`raw` / `compile/`）、設定 GUI の編集ドラフト（`draft/`）、コメントの読み出し（`comments`）、前回の設定ファイルの記憶（`last_used`） |
| `engine/` | フックの判断 — 修飾キーの論理状態、押下中のキー、2 ストローク、メニューガード、記憶キー、注入イベント。固定長・`Copy` の `Decision` を返す（ADR 0086） |
| `recorder.rs` | マクロ記憶の状態機械（ADR 0043） |
| `ime_indicator_settings.rs` | `[ime_indicator]` の設定値 |

**バイナリ（Win32 と UI）**

| モジュール | 責務 | unsafe |
|---|---|---|
| `main.rs` | 起動の順序（`cli` → 設定ファイルの決定 → 単一インスタンス → 読み込み → 常駐） | なし |
| `cli.rs` | コマンドライン引数の解析（副作用なし） | なし |
| `hook/` | コールバック（`mod`）、デバッグ行のリング（`debug`）、記憶イベントのリング（`record`）、単一インスタンス（`instance`）、メッセージループ（`message_loop`） | `mod` / `instance` / `message_loop` |
| `sender.rs` | `SendInput` のバッチ送出と注入マーカー（ADR 0006） | あり |
| `window.rs` | 前面ウィンドウの exe 名のキャッシュ（`SetWinEventHook`、ADR 0065） | あり |
| `layout.rs` | キーボード配列の読み取り（`MapVirtualKeyW` / `VkKeyScanW`、ADR 0063） | あり |
| `keyname.rs` | ログのキー名（ADR 0058） | あり |
| `clock.rs` | ローカル時刻（ADR 0041） | あり |
| `package.rs` | MSIX パッケージ判定と設定パスの解決（ADR 0061） | あり |
| `notify.rs` | コンソールの接続・切り離し・`--debug` の専用コンソール、通知ダイアログ（ADR 0029・0062・0068） | あり |
| `thread_msg.rs` | 機能スレッドへのメッセージとタイマー（ADR 0085） | あり |
| `worker.rs` | 機能スレッドの起動・停止（ADR 0085） | なし |
| `wide.rs` | UTF-16 への変換（§5-7） | なし |
| `cursor/` | IME 状態によるカーソルの着色と復元（`mod` / `scale` / `tint` / `gdi` / `pixels`。ADR 0067・0073・0076） | `pixels` 以外 |
| `ime_indicator/` | IME インジケーター（`mod` 司令、`decide` 判断、`detect` 照会、`overlay` 窓） | `detect` / `overlay` |
| `macro_record/` | マクロ記憶の再生スレッドとバナー（`mod` 司令、`banner` 窓） | `banner` |
| `gui/` | 設定ウィンドウ（`config_window/`）とログウィンドウ（`log`）、共有状態（`state`）、フォルダ監視（`watch`）、アイコン（`icons`）、Win32（`win32`） | `win32` |
| `i18n/` | UI 文言（日本語・英語。読み手ごとのファイル） | なし |
| `theme.rs` | 色と寸法（guidelines §12） | なし |
| `tray.rs` | タスクトレイ | なし |

unsafe を含むファイルの正本は [`tests/unsafe-allowlist.txt`](../tests/unsafe-allowlist.txt)（§5-3）。

### 使用クレート方針

- Win32 API: `windows`（windows-rs、Microsoft 公式）を使用。`winapi` クレートは使わない
- 設定: `serde` + `toml`
- エラー: `anyhow`（バイナリ）＋ `thiserror`（ライブラリ的部分）
- 依存は最小限に保つ。トレイ用クレート等を追加する場合はライセンス（MIT/Apache-2.0 系のみ可）を確認して ADR（Architecture Decision Record: 設計判断の記録）に記録する

---

## 5. 技術的制約・不変条件（違反禁止）

**正本は [AGENTS.md](../AGENTS.md) の「技術的不変条件」**（オーナー決定 2026-10-29、v1.1 開発計画 H3。それまでは本節が正本で AGENTS.md がその写しだったが、追記が AGENTS.md にしか入らない期間が続いたので、正本を入れ替えた）。ここには見出しだけを写す。矛盾があれば AGENTS.md が正。

1. **自己送出ループの防止** — `LLKHF_INJECTED` を確認し、注入イベントは素通し。例外は `test-inject` ビルドの `--accept-injected` のみ（ADR 0053）
2. **フックコールバック内の処理制限** — ヒープ確保・ロック・I/O・重い Win32 を禁止。明示的な例外は 4 つ（ADR 0016・0018・0020・0044）
3. **unsafe の隔離** — 許可ファイルは [`tests/unsafe-allowlist.txt`](../tests/unsafe-allowlist.txt)、各ブロックに `// SAFETY:`、`tests/invariants.rs` が検査（ADR 0083）
4. **抑止と送出の順序** — 抑止してから送出。リピートと keydown/keyup の対応を正しく扱い、modifier を壊さない
5. **既知の制限の明文化** — UIPI は回避せず README の Limitations に書く
6. **セキュリティ** — キー入力のログ保存をしない。デバッグログはキー名まで、既定 OFF
7. **文字コード** — UTF-8。Win32 との文字列変換は 1 か所（`src/wide.rs`）

---

## 6. 設定ファイル仕様（TOML・v0.1 案）

xremap の設定思想（アプリ別セクション + remap 定義）を TOML に写像する。詳細仕様は実装時に `docs/config-spec.md` として確定させること。

```toml
# %APPDATA%\winremap\config.toml

# グローバル（全アプリ適用）。application = ["*"] と等価
[[keymap]]
name = "global"
application = ["*"]

[keymap.remap]
# 例: CapsLock を Ctrl に（v0.1 では単キー→単キーのみ）
# "CapsLock" = "LCtrl"

# アプリ別: PHPStorm でのみ Ctrl+H → Backspace（本プロジェクトの発端）
[[keymap]]
name = "jetbrains-terminal-fix"
application = ["phpstorm64.exe"]

[keymap.remap]
"C-h" = "Back"
```

キー記法は fakeymacs / Keyhac ユーザーが読める形（`C-` = Ctrl、`A-` = Alt、`S-` = Shift、`W-` = Win、キー名は `Back`、`Enter`、`Esc`、`a`-`z`、`F1`-`F24` 等）を採用する。パーサは `keymap.rs` の純粋関数とし、不正な記法は行番号付きのエラーで報告する。

---

## 7. リポジトリ構成

v1.1（2026-10-29）時点。`src/` の各モジュールの責務は §4。

```
winremap/
├── .github/
│   ├── CODEOWNERS              # AGENTS.md / docs/ / workflows/ をオーナーレビュー必須に
│   └── workflows/              # ci.yml（fmt / clippy / test / build / site の一致）、release.yml、pages.yml
├── AGENTS.md                   # AI エージェント向け規約。不変条件と開発規約の正本（§5・§8）
├── CLAUDE.md                   # 「AGENTS.md を読むこと」のみ記載
├── README.md / README.ja.md    # 英語が正、日本語が追随
├── LICENSE / SECURITY.md / THIRD-PARTY-NOTICES.md / CHANGELOG.md
├── Cargo.toml / Cargo.lock / build.rs / .cargo/config.toml
├── assets/                     # アイコンの正本（svg/ ai/）と生成物（png/ .ico）、メニューの Bootstrap Icons
├── docs/
│   ├── 01_project-brief.md … 06_icon-assets.md   # アプリ全体の文書（README.md が索引）
│   ├── decisions-index.md      # ADR の通し索引
│   └── v0.1/ … v1.1/           # 版ごとの開発計画・受け入れ・decisions/・notes/
├── examples/                   # サンプル設定（minimal / emacs / personal-ja）、registry/、道具 2 本（app_icons / msix_assets）
├── installer/                  # Inno Setup
├── packaging/                  # winget / scoop / msix
├── site-src/ → site/           # ヘルプサイトの原稿と、Zola が生成してコミットする成果物（ADR 0066）
├── src/
│   ├── lib.rs                  # ライブラリ crate `winremap`: keymap/ config/ engine/ recorder.rs ime_indicator_settings.rs
│   ├── main.rs                 # バイナリ: cli.rs hook/ sender.rs window.rs layout.rs keyname.rs clock.rs package.rs
│   │                           #   notify.rs thread_msg.rs worker.rs wide.rs cursor/ ime_indicator/ macro_record/
│   │                           #   gui/ i18n/ theme.rs tray.rs
│   └── <module>/tests.rs       # フォルダ化したモジュールのテスト（guidelines §5）
└── tests/
    ├── keymap_test.rs          # 公開 API 越しの結合テスト（サンプル設定が常にパースできる）
    ├── invariants.rs           # 不変条件の機械検査（ADR 0083）
    ├── unsafe-allowlist.txt    # unsafe を書いてよいファイルの正本
    ├── acceptance/             # 受け入れの対話ハーネスと設定（ADR 0069・0070）
    └── ui/                     # VM での UI テスト（05_ui-test-automation.md）
```

**AGENTS.md の管理方針**: `AGENTS.md` は本リポジトリ内で完結する通常ファイルとして管理し、外部リポジトリへの参照・git submodule・シンボリックリンクによる取り込みは行わない。誰が clone しても追加の認証や依存なしに同じ内容が得られる状態を保つこと。

---

## 8. 開発規約（AIエージェント向け）

**正本は [AGENTS.md](../AGENTS.md)**（§5 と同じ理由で、2026-10-29 に正本を入れ替えた。v1.1 開発計画 H3）。指示ソースの限定、ワークフロー（必読文書、ADR、Conventional Commits、ブランチ運用、`cargo fmt` / `clippy` / `test`、テストの義務、英語の README とコメント、文書のメタ情報、ヘルプサイト、製品名の表記）、禁止事項、迷ったときの優先順位（**安定性 > 単純さ > 機能の豊富さ**）は、すべてそちらにある。

---

## 9. テスト方針

低レベルフックは CI（GitHub Actions のヘッドレス Windows）では実動テストできない。そのため次の分担とする。

- **CI で回す自動テスト**: キー記法パース、キーマップのマッチング（プロセス名 → 適用ルール解決）、TOML 設定の検証エラー。すべて純粋関数として `cargo test` 対象にする
- **手動受け入れテスト**（リリース前に人間が実施、`docs/` にチェックリスト化）:
  1. PHPStorm のターミナルで Ctrl+H が1文字削除になる（発端の問題）
  2. 他アプリ（メモ帳、ブラウザ）で Ctrl+H が変換されない
  3. 他のキーフック常駐ソフト（Keyhac 等）との併用確認（検証時は片方のフックを OFF にして切り替え比較する。二重フックは挙動が不定になる）
  4. 設定リロードがキー取りこぼしなく完了する
  5. 高速タイピング・キーリピート中に文字化け/二重入力が起きない

---

## 10. 公開運用とリリースの完全性

キーリマッパーは性質上、悪意ある改変版が「キーロガー」として出回った場合の被害が大きいジャンルである。次を必須の運用とする。

1. **ブランチ保護**: `main` への直接 push を禁止し、`AGENTS.md`・`docs/`・`.github/workflows/` への変更はオーナーの人間レビューを必須とする（CODEOWNERS で強制）。エージェントの指示書やビルドパイプラインが PR 経由で書き換えられる攻撃面を塞ぐため
2. **リリースの検証可能性**: GitHub Releases には exe と併せて `SHA256SUMS` を添付し、GitHub Artifact Attestations（ビルド来歴の証明）を有効化する。`SECURITY.md` に利用者向けの検証手順を記載する
3. **正規配布元の一元化**: 配布は GitHub Releases（将来的に crates.io / winget を追加する場合は ADR で判断）のみとし、README に「他サイトで配布されているバイナリは非公式」と明記する
4. **脆弱性報告**: `SECURITY.md` に GitHub の Private Vulnerability Reporting を窓口として記載する

---

## 11. マイルストーン

1. **M1**: `keymap.rs` + `config.rs`（パース・マッチング・テスト）— フック不要で完結、CI 緑化まで
2. **M2**: `hook.rs` + `sender.rs` + `window.rs` — minimal.toml で PHPStorm の Ctrl+H 問題が解決することを確認（受け入れテスト §3.1）
3. **M3**: `tray.rs` + ホットリロード + 単一インスタンス
4. **M4**: `examples/emacs.toml` 整備、README（en/ja）、CI/Release ワークフロー、ブランチ保護・CODEOWNERS 設定 → **v0.1.0 公開**
5. **M5 以降**: v0.2 候補（§3.2）を ADR で個別に判断

---

## 12. 参考リンク

- Keyhac: https://github.com/crftwr/keyhac-win （craftware 氏、MIT、v1.83）
- Keyhac 公式サイト: https://sites.google.com/site/craftware/keyhac-ja
- fakeymacs: https://github.com/smzht/fakeymacs （smzht 氏、MIT）
- 個人用 fakeymacs フォーク: https://github.com/DaikiSuganuma/fakeymacs
- xremap: https://github.com/xremap/xremap （設定思想・アーキテクチャの主参考）
- 発端のブログ記事: https://blog.dksg.jp/2026/06/claude-code-ctrlh-1.html
- Low-Level Keyboard Hook (MSDN): LowLevelKeyboardProc / KBDLLHOOKSTRUCT / SendInput のリファレンスを実装時に参照

---

*最終更新: 2026-10-29（v1.1: §4 のモジュール表と §7 の構成を分割後の `src/` に合わせ、§5・§8 の正本を AGENTS.md に移した）*
