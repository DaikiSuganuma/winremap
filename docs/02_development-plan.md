# winremap 開発計画（v0.1.0 まで）

> 元資料: [01_project-brief.md](./01_project-brief.md)（以下「ブリーフ」）。本計画はブリーフ §11 のマイルストーンを実タスクに分解したもの。
> Rust 実装の作法は [03_rust-guidelines.md](./03_rust-guidelines.md) を参照。

- 作成日: 2026-07-18
- 作成: Claude Code（AI モデル: claude-fable-5）／レビュー・承認: オーナー

---

## 全体方針

- フェーズは直列に進める（Phase N の完了条件を満たしてから N+1 へ）。各フェーズ完了時点でオーナーのレビュー・受け入れ判断を挟む
- ロジック層（keymap / config）を先に純粋関数として完成させ、CI で緑化してから unsafe な Win32 層に着手する（ブリーフ §9 のテスト分担に対応）
- 設計判断が発生するたびに `docs/decisions/` に ADR を追加する。各フェーズの「想定 ADR」を下に列挙

---

## Phase 0 — プロジェクト基盤（ブリーフ M1 の前提整備）

コードを書く前に、規約とビルド・CI の土台を作る。

### タスク

1. `AGENTS.md` 作成 — ブリーフ §5（不変条件）・§8（開発規約・指示ソースの限定）を反映した自己完結版
2. `CLAUDE.md` 作成 —「AGENTS.md を読むこと」のみ記載
3. `cargo init`（バイナリクレート、edition は最新安定を選択し ADR に記録）、`.gitignore`（`target/`, `.mcp.json`, `*.log`）
4. `Cargo.toml` に初期依存を宣言: `serde` + `toml` + `anyhow` + `thiserror`（`windows` は Phase 2 で追加）
5. `.github/workflows/ci.yml` — `fmt` / `clippy -- -D warnings` / `test` / `build`（windows-latest）
6. `docs/decisions/0001-use-windows-rs.md`（winapi 不採用の理由を含む）、`docs/decisions/0002-rust-edition-2024.md`（edition・ツールチェーン方針）
7. `CHANGELOG.md`（Keep a Changelog 形式、Unreleased のみ）
8. `docs/03_rust-guidelines.md` — Rust 開発の作法（エラー処理・unsafe 規約・テスト・依存管理等）

### 完了条件

- 空の `main.rs` で CI が全ジョブ緑
- AGENTS.md / CLAUDE.md をオーナーがレビュー済み

---

## Phase 1 — ロジック層（= ブリーフ M1: keymap.rs + config.rs）

フック不要で完結する純粋ロジックを実装し、テストで固める。

### タスク

1. `docs/04_config-spec.md` 確定 — ブリーフ §6 の TOML 案を仕様化。キー記法（`C-` / `A-` / `S-` / `W-`、`Back` / `Enter` / `Esc` / `a`-`z` / `F1`-`F24` 等）と VK コード対応表を定義
2. `keymap.rs`
   - キー記法パーサ（文字列 → modifier ビット + VK）。不正記法は位置情報付きエラー
   - マッチング: (前面プロセス名, 入力キーイベント) → 適用リマップ解決。アプリ別定義がグローバル `*` より優先されるルールを仕様化
   - プロセス名比較は大文字小文字非依存（Windows の exe 名慣習）
3. `config.rs`
   - TOML 読み込み・検証。エラーは行番号付きで報告
   - フック層から参照する読み取り専用構造（ルックアップ最適化済み）への変換。リロード時の atomic swap を見越し `arc-swap` を導入（ADR）
4. `tests/keymap_test.rs` + 各モジュールの単体テスト
   - パース正常系/異常系、マッチング優先順位、`examples/minimal.toml` 相当の設定が期待どおり解決されること

### 想定 ADR

- 0003: 設定リロードの atomic swap 方式（arc-swap 採用）
- 0004: キーマップ優先順位ルール（アプリ別 > グローバル、同一アプリ内の複数キーマップの扱い）

### 完了条件

- `cargo test` 緑（CI 上）。`examples/minimal.toml` を作成しパーステストが通る
- キー入力を一切扱わずにここまで完了していること（Win32 依存ゼロ）

---

## Phase 2 — フック層（= ブリーフ M2: hook.rs + sender.rs + window.rs）

MVP の核心。ブリーフ §5 の不変条件を満たす実装が最重要。

### タスク

1. `windows` クレート追加（必要 feature のみ有効化）
2. `window.rs` — 前面ウィンドウ exe 名取得（`GetForegroundWindow` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW`）。`SetWinEventHook`（EVENT_SYSTEM_FOREGROUND）でフォアグラウンド変化時にキャッシュ更新し、フックコールバック内では API を呼ばない
3. `hook.rs` — `WH_KEYBOARD_LL` 設置/解除
   - `LLKHF_INJECTED` チェックによる自己送出素通し（不変条件 §5-1）
   - コールバック内はキャッシュ済み exe 名 + arc-swap 済み設定の参照のみ。ヒープ確保・ロック待ち・I/O 禁止（§5-2）
   - keydown / keyup / キーリピートの対応関係を正しく処理（§5-4）
4. `sender.rs` — `SendInput` ラッパー
   - **modifier の一時解除**: `C-h` → `Back` 置換時、物理 Ctrl が押下中のままだとアプリには Ctrl+Backspace が届いてしまう。置換キー送出前に押下中 modifier の解放イベントを注入し、送出後に復元する方式を設計・実装する（fakeymacs が行っている処理に相当。設計は ADR に記録）
   - 注入イベントへの識別情報付与（`dwExtraInfo`）
5. `main.rs` — メッセージループ、`--config` 引数、`%APPDATA%\winremap\config.toml` 既定パス
6. unsafe はこの 3 ファイルのみ、全 unsafe に `// SAFETY:` コメント（§5-3）

### 想定 ADR

- 0005: modifier 一時解除の送出シーケンス設計
- 0006: 注入イベント識別方法（dwExtraInfo マーカー + LLKHF_INJECTED の併用）

### 完了条件（= ブリーフ §3.1 受け入れテスト）

- `minimal.toml`（PHPStorm のみ `C-h` → `Back`）で:
  1. PHPStorm ターミナルの Claude Code で Ctrl+H が 1 文字削除になる
  2. メモ帳・ブラウザでは Ctrl+H が変換されない
  3. 高速タイピング・キーリピートで二重入力・取りこぼしがない
- 手動テスト結果を `docs/05_acceptance-checklist.md` に記録（Phase 4 でリリース用チェックリストに発展させる）

---

## Phase 3 — 常駐機能（= ブリーフ M3: tray.rs + ホットリロード + 単一インスタンス）

### タスク

1. トレイ用クレート選定（MIT/Apache-2.0 系のみ。候補を比較し ADR へ）
2. `tray.rs` — 有効/無効トグル、設定リロード、設定ファイルを開く、終了
3. ホットリロード — トレイメニューからのリロード + ファイル監視（監視方式・クレートは ADR）。リロードは arc-swap でキー取りこぼしなし（§9-4）
4. 単一インスタンス保証（named mutex）
5. 設定エラー時の挙動確定: 起動時エラーは通知して終了 or 空マップで常駐、リロード時エラーは旧設定を維持して通知（仕様として 04_config-spec.md に明記）

### 想定 ADR

- 0007: トレイクレート選定
- 0008: ファイル監視方式と設定エラー時のフォールバック挙動

### 完了条件

- トレイからの全操作が動作し、リロード中もキー入力が欠落しない（手動確認）
- 二重起動が防止される

---

## Phase 4 — 公開準備（= ブリーフ M4 → v0.1.0）

### タスク

1. `examples/emacs.toml` — fakeymacs 相当の Emacs 風設定（v0.1 の機能範囲＝単キー置換のみで表現できる範囲に限定。シーケンス等 v0.2 機能は含めない）
2. `README.md`（英語・正）/ `README.ja.md`（日本語・追随）
   - 一行説明、Quick Start、Limitations（UIPI：管理者権限ウィンドウには効かない等）、Acknowledgments（Keyhac / fakeymacs / xremap）、「Keyhac の再実装ではない」「xremap 非公式」の明記、「他サイト配布バイナリは非公式」の明記
   - レイテンシ改善は「最悪値と安定性の改善」と正確に記述（誇張禁止、ブリーフ §1.3）
3. `SECURITY.md` — Private Vulnerability Reporting 窓口、SHA256 / attestation 検証手順
4. `.github/workflows/release.yml` — タグ push → exe + SHA256SUMS + Artifact Attestations
5. `.github/CODEOWNERS` — AGENTS.md / docs/ / workflows/ をオーナーレビュー必須に
6. ブランチ保護設定（main 直接 push 禁止）— **GitHub 設定はオーナー作業**。手順書をエージェントが用意
7. `docs/05_acceptance-checklist.md` 完成版で手動受け入れテスト全 5 項目（ブリーフ §9）を実施・記録
8. `CHANGELOG.md` 0.1.0 化 → タグ `v0.1.0` → GitHub Release

### 完了条件

- 受け入れテスト 5 項目すべて合格（Keyhac 併用確認含む）
- v0.1.0 が GitHub Releases に SHA256SUMS + attestation 付きで公開

---

## Phase 5 以降（= ブリーフ M5）

v0.2 候補（キーシーケンス、tap/hold、マークモード、ウィンドウタイトル切替、設定 GUI）は着手前に 1 機能 1 ADR で個別判断。本計画のスコープ外。

---

## 横断的リスクと対策

| リスク | 対策 |
|---|---|
| modifier 一時解除の送出順序ミスで modifier 状態が壊れる | Phase 2 で最重要設計項目として ADR 化。受け入れテストに高速タイピング項目を含める |
| フックコールバック内での禁止操作（ヒープ確保等）の混入 | AGENTS.md に明記し、レビュー観点に固定。ログはフック外スレッドへ |
| 他常駐リマッパー（Keyhac）との二重フックで挙動不定 | 検証時は片方 OFF。README Limitations に併用非推奨を記載 |
| CI（ヘッドレス）でフック実動テスト不可 | ロジック層に薄いフック層を徹底し、手動チェックリストで補完（ブリーフ §9 どおり) |
| LGPL（kanata）コード混入 | 参照はアイデアレベルに限定。移植時は MIT プロジェクトのみ + THIRD-PARTY-NOTICES.md |

## オーナー判断が必要な項目（都度確認）

- Phase 0: AGENTS.md の内容承認
- Phase 3: トレイクレートの選定承認（ライセンス確認込み）
- Phase 4: ブランチ保護・CODEOWNERS の GitHub 設定作業、v0.1.0 リリース判断
