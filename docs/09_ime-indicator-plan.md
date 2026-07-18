# winremap IME 状態インジケーター開発計画

> 設計は [08_ime-indicator-design.md](08_ime-indicator-design.md)（以下「設計書」）、採用判断は [ADR 0020](decisions/0020-ime-indicator-scope.md) を参照。
> 本計画は [02_development-plan.md](02_development-plan.md) と同じ運用（フェーズ直列、フェーズ完了ごとにオーナーレビュー）に従う。

- 作成日: 2026-07-19
- 作成: Claude Code（AI モデル: claude-fable-5）／レビュー・承認: オーナー

---

## 全体方針

- フェーズは直列に進める。**Phase I1 の技術検証が Go にならない限り本実装（Phase I2 以降）に着手しない**
- 本機能のコードは `src/ime_indicator/` に隔離し、既存モジュールへの接点は通知 1 行レベルに保つ（設計書 §4）。レビューで接点の増殖をチェックする
- 既定無効（opt-in）を全フェーズで維持し、`enabled = false` の利用者にはコード追加の影響が一切ないことを完了条件に含める

---

## Phase I0 — 規約・文書の整備

実装前に、本機能がプロジェクト規約と矛盾しない状態を作る。

### タスク

1. ~~ADR 0020 作成（統合判断・却下代替案の記録）~~ — 済（2026-07-19）
2. ~~設計書（docs/08）・本計画書（docs/09）作成~~ — 済（2026-07-19）
3. ~~ブリーフ §3.3 Non-goals の改訂（「表示のみ採用、制御は引き続き Non-goal」）~~ — 済（2026-07-19、オーナーレビュー待ち）
4. ~~`AGENTS.md` 改訂~~ — 済（2026-07-19、オーナー承認済み）:
   - 不変条件 2 の明示的例外に「ime_indicator 有効時のトグル候補キー検知での `PostThreadMessageW`（ADR 0020）」を追加
   - 不変条件 3 の unsafe 許可リストに `src/ime_indicator/detect.rs` / `src/ime_indicator/overlay.rs`・検証用 `examples/ime_probe.rs` を追加
5. ~~ブリーフ §5（不変条件）にも同じ改訂を反映~~ — 済（2026-07-19）

### 完了条件

- 上記文書一式をオーナーがレビュー・承認済み

---

## Phase I1 — 技術検証（Go/No-Go ゲート）

最重要リスク「モダン IME で `IMC_GETOPENSTATUS` が機能しない」（設計書 §6-1）を、本実装前にオーナー環境で検証する。

### タスク

1. ~~検証用バイナリ `examples/ime_probe.rs` を作成~~ — 済（2026-07-19。`cargo run --example ime_probe` で実行。起動と照会動作は確認済み、µs オーダーで応答）
   - 1 秒間隔でフォアグラウンドウィンドウの IME 状態を照会（`ImmGetDefaultIMEWnd` → `SendMessageTimeoutW(WM_IME_CONTROL, IMC_GETOPENSTATUS)`）し、exe 名・HWND・戻り値・所要時間をコンソールに表示する
   - 本体のフック・設定には一切触れない独立コード（検証後も回帰確認用に残す）
2. オーナー環境（Windows 11 Pro + 常用 IME）で以下を記録する:
   - メモ帳 / ブラウザ / PHPStorm / Windows Terminal 各アプリで、IME オン/オフの切替が戻り値に正しく反映されるか
   - 半角/全角キー押下から状態反映までの遅延（設計書 §3.1 の照会遅延 50ms の妥当性確認）
   - トグル候補キーの実測 VK コード（`--debug` の既存キーログで確認し、設計書 §3.1 の候補集合を確定）
3. 結果を本計画書の末尾（または `docs/notes/`）に記録する

### Go/No-Go 判断（オーナー）

- **Go**: 常用アプリで状態が正しく取れる → Phase I2 へ
- **No-Go**: 戻り値が常に 0 等 → 以下から選択して ADR を追加
  - a. 「以前のバージョンの Microsoft IME」設定を前提条件として README に明記した上で採用
  - b. 機能を見送り（文書は「見送り」ステータスで保存）
  - c. TSF 等の代替手段を別途調査

### 完了条件

- 検証結果が記録され、オーナーが Go/No-Go を判断済み

---

## Phase I2 — 設定・純ロジック層（CI で完結）

### タスク

1. `docs/04_config-spec.md` に `[ime_indicator]` 節の仕様を追記（設計書 §3.4 の確定版）
2. ライブラリクレート側に `[ime_indicator]` のパース・検証・既定値を実装
   - 範囲チェック（duration_ms 100–5000、size 32–256、opacity 0–255）、範囲外は行番号付きエラー
   - 節省略時は `enabled = false` の既定値構造体を返す
3. テスト追加: パース正常系・異常系・既定値・リロード時の設定差し替え（既存の config テストの流儀に合わせる）

### 完了条件

- `cargo test` 緑（CI 上）。Win32 依存ゼロでここまで完了していること（02_development-plan.md Phase 1 と同じ原則）

---

## Phase I3 — 本体実装（src/ime_indicator/）

### タスク

1. `src/ime_indicator/mod.rs` — 公開 API（`start` / `stop` / 通知関数）、indicator thread の起動・停止・設定リロード受信
2. `src/ime_indicator/detect.rs` — IME 状態照会（設計書 §3.2）。全 unsafe に `// SAFETY:` コメント
3. `src/ime_indicator/overlay.rs` — レイヤードウィンドウ生成・「あ」パネル描画・フェードアウトタイマー（設計書 §3.3）
4. 既存モジュールへの接点追加（各 1 箇所、レビューで接点数を確認）:
   - `hook.rs`: トグル候補キー keydown 時の `PostThreadMessageW`（enabled 時のみ。キーは素通し）
   - `window.rs`: フォアグラウンド変化通知
   - `main.rs`: 起動・終了・リロード時の配線
5. `Cargo.toml` の `windows` クレート feature 追加（`Win32_UI_Input_Ime`、`Win32_Graphics_Gdi`、`Win32_Graphics_Dwm` 等、必要最小限）
6. DPI awareness マニフェストの現状確認と、必要なら Per-Monitor V2 対応（結果を ADR に記録）
7. 障害分離の実装確認: indicator thread の起動失敗・panic がリマップ動作に影響しないこと

### 想定 ADR

- 0021: indicator thread の通信設計（メッセージ種別、設定リロード伝搬）※実装時に判断が発生した場合
- 0022: DPI awareness 方針 ※マニフェスト変更が必要になった場合

### 完了条件

- `cargo fmt` / `cargo clippy -- -D warnings` 緑
- `enabled = false`（既定）でビルド・実行したとき、フック経路に追加コストがない（トグルキー検知の分岐が enabled チェックで即座に抜ける）ことをコードレビューで確認
- 開発機で設計書 §7 の手動項目 1–5 が通る

---

## Phase I4 — 受け入れ・公開

### タスク

1. `docs/05_acceptance-checklist.md` に設計書 §7 の手動受け入れ項目を追加し、全項目を実施・記録
2. README（英語・正）/ README.ja.md 更新:
   - 機能紹介（opt-in であること、設定例）
   - Limitations 追記: モダン IME 非互換の可能性と対処、UIPI（管理者権限ウィンドウでは表示されない）、フルスクリーン排他アプリ、Microsoft IME 以外は対象外
3. `CHANGELOG.md` 更新（Keep a Changelog 形式、Added）
4. バージョン判断（オーナー）: 機能追加のため v0.x のマイナーバージョンアップを想定
5. タグ push → リリース（既存の release.yml 運用どおり）

### 完了条件

- 受け入れ項目全合格、リリース公開、README(en/ja) 同期済み

---

## 横断的リスクと対策

| リスク | 対策 |
|---|---|
| モダン IME で照会 API が機能しない | Phase I1 の Go/No-Go ゲートで本実装前に判定。No-Go 時の選択肢を事前定義 |
| `SendMessage` ブロックによるハング | `SendMessageTimeoutW` 限定（設計書 §3.2）。無印 `SendMessage` はレビューで差し戻し |
| フック内処理の肥大化（不変条件 2 違反） | フック内は enabled チェック＋ `PostThreadMessageW` 1 行のみ。AGENTS.md の例外リストに明記してレビュー観点に固定 |
| インジケーター不具合がリマップを巻き込む | 専用スレッドで障害分離（設計書 §5）。panic 時は機能停止のみでフック継続 |
| 描画・DPI の環境差 | 受け入れチェックリストにマルチモニター・DPI 混在項目を追加。CI 不可領域は手動で補完（既存方針どおり） |
| 「別機能」の境界が崩れて本体と癒着する | 接点 3 箇所（hook / window / main 各 1 行レベル）を上限としてレビューで監視 |

## オーナー判断が必要な項目（都度確認）

- Phase I0: AGENTS.md / ブリーフ改訂の承認
- Phase I1: Go/No-Go 判断（検証結果に基づく）
- Phase I4: バージョン番号とリリース判断

---

## 検証記録（Phase I1 実施時に記入）

（未実施）
