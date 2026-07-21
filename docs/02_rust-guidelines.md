# Rust 開発の作法（WinRemap）

> 本プロジェクトで Rust コードを書く際の規約。[01_project-brief.md](./01_project-brief.md) §4（アーキテクチャ）・§5（不変条件）を前提とし、Rust 実装面の具体的な作法を定める。矛盾がある場合はブリーフ §5 が優先。
> 各規約の根拠となる公式資料は末尾の [§12 参考資料](#12-参考資料公式) を参照。
> 文中の ADR は Architecture Decision Record（設計判断の記録、各バージョンフォルダの `docs/<version>/decisions/` 配下）を指す。

- 作成日: 2026-07-18
- 作成: Claude Code（AI モデル: claude-fable-5）／レビュー・承認: オーナー

---

## 1. ツールチェーンとビルド

- **チャネル**: stable のみ。nightly 機能は使用しない
- **edition**: 2024（[ADR 0002](./v0.1/decisions/0002-rust-edition-2024.md)）
- **MSRV**: 最新 stable に追随（CI が stable を使うため、明示的な MSRV 保証はしない。crates.io 公開を検討する段階で ADR により再判断）
- コミット前に必ず通すこと:
  ```
  cargo fmt --all
  cargo clippy --all-targets -- -D warnings
  cargo test
  ```
- rustfmt / clippy とも設定ファイルによるカスタマイズはしない（デフォルト運用）。`#[allow]` が必要な場合は必ず理由コメントを添え、関数単位より広いスコープに付けない

## 2. エラー処理

- **`anyhow`**: バイナリの起動経路（`main.rs` の初期化、設定ファイル読み込みの呼び出し側）で使用。文脈は `.context()` / `.with_context()` で必ず付与する
- **`thiserror`**: ライブラリ的モジュール（`keymap.rs` / `config.rs`）のエラー型定義に使用。呼び出し側が分岐できる粒度で variant を切る
- **`unwrap()` / `expect()` の禁止**: 実行時経路では使用しない。不変条件により絶対に失敗しない箇所のみ `expect("理由")` を許可（理由必須）。テストコード内は自由
- **`panic!` 禁止（特にフック経路）**: `hook.rs` のコールバックから到達しうるコードでは panic を絶対に起こさない。FFI 境界（Windows が呼ぶコールバック）を panic が越えるとプロセス異常終了・未定義動作になる。コールバック本体は panic フリーに書き、論理エラーは「変換せず素通し」に倒す（安定性 > 機能、ブリーフ §8）
- 設定パースのエラーは**位置情報（行番号・キー名）付き**でユーザーに報告する

## 3. unsafe の規約

- `unsafe` を書いてよいのは `hook.rs` / `sender.rs` / `window.rs` の 3 ファイルのみ（ブリーフ §5-3）
- すべての `unsafe` ブロック直前に `// SAFETY:` コメントで安全性の根拠を記載（clippy の `undocumented_unsafe_blocks` 相当の運用）
- unsafe ブロックは**最小スコープ**にする。Win32 API 呼び出し 1 つにつき 1 ブロックが原則。複数呼び出しをまとめて包まない
- unsafe な API を安全な Rust 型（所有権・Result）で包むラッパー関数を作り、モジュール外には安全なインターフェースのみ公開する

## 4. フック経路のパフォーマンス制約

ブリーフ §5-2 の Rust 実装ルール。`LowLevelKeyboardProc` コールバックから到達するコードでは:

- **禁止**: ヒープ確保（`String` / `Vec` / `Box` の生成、`format!`）、ブロックするロック（`Mutex` / `RwLock`）、ファイル・コンソール I/O、チャネル送信のブロック待ち、重い Win32 API 呼び出し
- **許可**: 事前構築済み読み取り専用構造の参照、`arc-swap` の load、整数演算・配列参照
- 設定データはリロード時に**フック外のスレッド**で構築し、`arc_swap::ArcSwap` で差し替える。フック側は `load()` のみ
- 前面プロセス名はフォアグラウンド変化イベント（`SetWinEventHook`）で更新されるキャッシュを参照する。フック内で `GetForegroundWindow` 等を呼ばない
- デバッグログが必要な場合はロックフリーな手段（固定長リングバッファ等）でフック外スレッドに渡して出力する。実装するときは ADR を書く

## 5. モジュール構成と可視性

- モジュール責務はブリーフ §4 の表に従う。責務をまたぐ実装をしない（例: `keymap.rs` から Win32 API を呼ばない）
- **1 ファイルの記述量が増えたら構造化する**（目安: テスト込みで 300 行超、または責務が複数見えてきたとき）。モジュールをフォルダ化し（`config.rs` → `config/mod.rs` + サブモジュール）、公開 API は `mod.rs` の `pub use` で維持する
- **`#[cfg(test)]` のテストコードはフォルダ化したモジュールでは `tests.rs` に分離**し、実装ファイルに混在させない
- `keymap.rs` / `config.rs` は **OS 非依存の純粋ロジック**として書き、`#[cfg(windows)]` を含めない。VK コードは `u16` 等のプレーンな型で扱う
- 公開範囲は最小に。`pub` はモジュール間インターフェースのみ、内部ヘルパーは非公開
- 共通の文字列変換（UTF-8 ↔ UTF-16）はヘルパーを 1 箇所に集約する（ブリーフ §5-7）

## 6. 命名とコメント

- Rust 標準の命名規約（`snake_case` / `UpperCamelCase` / `SCREAMING_SNAKE_CASE`）。clippy に従う
- **コード内コメントは英語で書く**（GitHub で公開するため。ブリーフ §8）
- コメントは**「何を」ではなく「なぜ」を書く**。コードを読めば分かることは書かない
- 非自明な分岐・集計・並べ替え・フォールバック・回避策には、意図を一言添える
- `TODO` / `FIXME` には簡潔な理由を添える（例: `// TODO: support OEM keys — layout-dependent, needs JP keyboard testing`）
- doc comment（`///`）は公開アイテムに付け、「何をするか」に加えて「どんな制約・前提があるか」を書く
- Win32 の定数・構造体名は windows-rs の命名をそのまま使い、独自の別名を作らない

## 7. テスト

- 単体テストは各モジュール内の `#[cfg(test)] mod tests`、結合テスト（設定ファイル全体のパース〜マッチング解決）は `tests/` 配下
- `keymap.rs` / `config.rs` の変更には**必ずテストを伴わせる**（ブリーフ §8）。フック層（`hook.rs` / `sender.rs` / `window.rs`）は CI で実動不可のためテスト免除だが、その分ロジックを持たせない
- テストは正常系だけでなく**異常系（不正なキー記法、壊れた TOML、重複定義）**を必ず含める
- テストデータに `examples/` の実ファイルを使い、サンプル設定が常にパース可能であることを保証する

## 8. 依存クレートの管理

- 依存は最小限。追加時は次を確認し **ADR を 1 件書く**:
  1. ライセンスが MIT / Apache-2.0 系であること（LGPL / GPL は不可）
  2. メンテナンス状況（最終リリース、issue 対応）
  3. 推移的依存の量
- `windows` クレートは**必要な feature のみ**有効化する（ビルド時間と攻撃面の最小化）
- ネットワーク通信を行うクレートを追加しない（ブリーフ §8 禁止事項）
- `Cargo.lock` はコミットする（バイナリ配布物の再現性確保）

## 9. セキュリティ実装ルール

- キー入力内容（打鍵されたキーの列）を永続化するコードを書かない。デバッグ出力もキー名レベルまでとし、既定 OFF（ブリーフ §5-6）
- `dwExtraInfo` 等に埋め込む自己識別マーカーは定数 1 箇所で定義する
- 外部入力（設定ファイル）のパースは serde/toml に任せ、独自のパーサを書かない（キー記法パーサは例外で、`keymap.rs` に純粋関数として実装）

## 10. コミットと CI

- Conventional Commits（`feat:` / `fix:` / `docs:` / `refactor:` / `test:` / `chore:` / `ci:`）。1 コミット 1 関心事
- fmt / clippy / test が通らないコードをコミットしない（CI でも強制）
- CI は `.github/workflows/ci.yml`（windows-latest）: fmt チェック → clippy `-D warnings` → test → release build

## 11. UI テキストの多言語化

- ユーザーに見える UI テキスト（トレイメニュー、ツールチップ、コンソールの案内文、CLI ヘルプ）は **`src/i18n.rs` を必ず経由**し、コードにハードコードしない。対応言語は日本語・英語（[ADR 0014](./v0.1/decisions/0014-ui-localization.md)）
- 言語はシステムの UI 言語から自動選択（`ja*` → 日本語、それ以外 → 英語）。`--lang en|ja` で上書き可
- エラーの技術的詳細（設定検証エラー、`anyhow` の context 文字列）は診断情報として**英語のまま**とする。テストが文言を検証していることと、Issue 報告時にそのまま貼れる利点のため
- 新しい UI 文言を追加するときは英語・日本語の両方を同時に定義する（片方だけの追加はレビューで差し戻し）

## 12. 参考資料（公式）

本ドキュメントの規約の根拠・参照先。実装時に迷ったらここに戻る。

### 言語・edition（§1, §3）

- The Rust Programming Language（The Book）: https://doc.rust-lang.org/book/
- Rust Edition Guide — Rust 2024: https://doc.rust-lang.org/edition-guide/rust-2024/index.html
- `unsafe_op_in_unsafe_fn`（edition 2024 で既定化）: https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html
- The Rustonomicon（unsafe Rust の公式ガイド）: https://doc.rust-lang.org/nomicon/
- SAFETY コメントの運用（Rust 標準ライブラリ開発ガイド）: https://std-dev-guide.rust-lang.org/policy/safety-comments.html

### API 設計・命名（§5, §6）

- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- 同 Naming 章: https://rust-lang.github.io/api-guidelines/naming.html

### ツール（§1, §10）

- rustfmt: https://github.com/rust-lang/rustfmt
- Clippy lint 一覧（`undocumented_unsafe_blocks` 等）: https://rust-lang.github.io/rust-clippy/master/
- Cargo Book — FAQ「Cargo.lock をバージョン管理に含める理由」: https://doc.rust-lang.org/cargo/faq.html#why-have-cargolock-in-version-control
- Conventional Commits: https://www.conventionalcommits.org/

### テスト（§7）

- The Book — Test Organization: https://doc.rust-lang.org/book/ch11-03-test-organization.html

### 依存クレート（§2, §4, §8）

- anyhow: https://docs.rs/anyhow
- thiserror: https://docs.rs/thiserror
- arc-swap: https://docs.rs/arc-swap
- windows-rs（Microsoft 公式）: https://github.com/microsoft/windows-rs
- Rust for Windows（Microsoft Learn）: https://learn.microsoft.com/windows/dev-environment/rust/

### Win32 API（§3, §4）

- LowLevelKeyboardProc: https://learn.microsoft.com/windows/win32/winmsg/lowlevelkeyboardproc
- KBDLLHOOKSTRUCT: https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-kbdllhookstruct
- SendInput: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput
