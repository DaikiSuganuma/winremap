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
2. ~~オーナー環境（Windows 11 Pro + 常用 IME）で記録~~ — 済（2026-07-19、末尾の検証記録参照。トグルキーの VK 実測のみ Phase I3 へ持ち越し）:
   - メモ帳 / ブラウザ / PHPStorm / Windows Terminal 各アプリで、IME オン/オフの切替が戻り値に正しく反映されるか
   - 半角/全角キー押下から状態反映までの遅延（設計書 §3.1 の照会遅延 50ms の妥当性確認）
   - トグル候補キーの実測 VK コード（`--debug` の既存キーログで確認し、設計書 §3.1 の候補集合を確定）
3. ~~結果を本計画書の末尾に記録する~~ — 済（2026-07-19、Go 判定）

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

1. ~~`docs/04_config-spec.md` に `[ime_indicator]` 節の仕様を追記~~ — 済（2026-07-19、config-spec §6）
2. ~~ライブラリクレート側に `[ime_indicator]` のパース・検証・既定値を実装~~ — 済（2026-07-19。設定型は `src/ime_indicator_settings.rs` に分離し、`RemapTable` に載せて `macro_delay_ms` と同様にリロードで一括差し替え）
   - 範囲チェック（duration_ms 100–5000、size 32–256、opacity 0–255）、範囲外は行番号付きエラー
   - 節省略時は `enabled = false` の既定値構造体を返す
3. ~~テスト追加~~ — 済（2026-07-19。既定値・全指定・部分指定・範囲外 3 件一括報告・未知フィールドの 5 テスト）

### 完了条件

- `cargo test` 緑（CI 上）。Win32 依存ゼロでここまで完了していること（02_development-plan.md Phase 1 と同じ原則）

---

## Phase I3 — 本体実装（src/ime_indicator/）

### タスク

1. ~~`src/ime_indicator/mod.rs`~~ — 済（2026-07-19。公開 API は `sync_with_config` / `stop` / `notify_*`。unsafe なしの制御ロジックに限定）
2. ~~`src/ime_indicator/detect.rs`~~ — 済（2026-07-19。`SendMessageTimeoutW` のみ使用、全 unsafe に `// SAFETY:`）
3. ~~`src/ime_indicator/overlay.rs`~~ — 済（2026-07-19。レイヤードウィンドウ・「あ」パネル・フェードアウトに加え、スレッドメッセージングの unsafe ラッパーもここに集約し mod.rs を unsafe フリーに保った）
4. ~~既存モジュールへの接点追加~~ — 済（2026-07-19。hook.rs / window.rs / main.rs に加え、リロードがトレイ経由のため tray.rs の計 4 箇所・各 1 行レベル。設計書 §4 に反映済み）
5. ~~`Cargo.toml` feature 追加~~ — 済（2026-07-19。`Win32_Graphics_Gdi` / `Win32_Graphics_Dwm` / `Win32_System_LibraryLoader` を追加）
6. ~~DPI awareness マニフェストの現状確認~~ — 済（2026-07-19。build.rs にマニフェスト設定はなく DPI 非対応＝OS の DPI 仮想化に任せる構成。座標系が仮想化内で一貫するため中央配置は正しく動作し、変更不要と判断。高 DPI での描画シャープ化は将来課題。変更なしのため ADR は起こさず）
7. ~~障害分離の実装確認~~ — 済（2026-07-19。indicator thread は `catch_unwind` で包み、起動失敗・panic とも i18n 警告のみでリマップ継続）

### 想定 ADR

- ~~indicator thread の通信設計~~ — 不要と判断(設計書 §4.1 の記載どおりに実装、新たな設計判断は発生せず)
- ~~DPI awareness 方針~~ — マニフェスト変更は不要（タスク 6 参照）だったが、検証で DWM 矩形の物理ピクセル問題が発覚し [ADR 0022](decisions/0022-ime-indicator-getwindowrect.md) として記録
- 検証で発覚した問題への対処として [ADR 0021（トリガーキー設定化）](decisions/0021-ime-indicator-trigger-keys.md)・[ADR 0022（GetWindowRect 統一）](decisions/0022-ime-indicator-getwindowrect.md) を追加（下記検証記録参照）

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
| 「別機能」の境界が崩れて本体と癒着する | 接点 4 箇所（hook / window / main / tray 各 1 行レベル。リロードがトレイ経由のため tray を追加）を上限としてレビューで監視 |

## オーナー判断が必要な項目（都度確認）

- Phase I0: AGENTS.md / ブリーフ改訂の承認
- Phase I1: Go/No-Go 判断（検証結果に基づく）
- Phase I4: バージョン番号とリリース判断

---

## 検証記録（Phase I1）

- 実施日: 2026-07-19／実施: オーナー（Windows 11 Pro 10.0.26200）／記録: Claude Code（AI モデル: claude-fable-5）
- 手順: `cargo run --example ime_probe` を実行し、アプリを切り替えながら半角/全角キーで IME をトグル

### 結果: **Go**（オーナー判断 2026-07-19）

| 確認項目 | 結果 |
|---|---|
| phpstorm64.exe での IME トグル追随 | **OK** — `open=OFF → ON → OFF` が切替のたびに正しく反映（`*` マーク付きで遷移を検出） |
| windowsterminal.exe での照会 | 照会自体は成功（`open=OFF`）。トグル追随のログは未取得 |
| 照会レイテンシ | 24〜1582 µs（多くは数十〜数百 µs）。設計のタイムアウト 100ms に対し十分小さい |
| `ImmGetDefaultIMEWnd` | 両アプリで有効な IME ウィンドウハンドルを返却 |

モダン IME 環境（Windows 11 Pro 26200）で `IMC_GETOPENSTATUS` が実状態を返すことを確認。設計書 §6-1 の「常に 0」非互換はこの環境では発生しない。

### 残タスク（Phase I3 で実施）

- トグル候補キーの実測 VK コード確認（`winremap --debug` のキーログで採取し、設計書 §3.1 の候補集合を確定）
- Windows Terminal・メモ帳・ブラウザでのトグル追随の再確認（受け入れテスト時に実施）

## 検証記録（Phase I3・1 回目: 2026-07-19）

- 実施: オーナー（release ビルド + `--debug` + examples/suganuma.toml、`[ime_indicator] enabled = true` 設定済み）
- 結果: **NG — パネルが一切表示されない**

### 原因分析（デバッグログより）

1. **トリガー不発（主因）**: オーナーの IME 切替は Ctrl+Space（Windows 11 IME のオプション）で、ログに `C-Space → 素通し` が多数記録されている。ハードコードされたトグル候補 VK セットに Ctrl+Space が含まれず、フックから indicator スレッドへの通知が飛んでいなかった → **[ADR 0021](decisions/0021-ime-indicator-trigger-keys.md): `trigger_keys` 設定を追加**（suganuma.toml に `["C-Space"]` を設定）
2. **位置計算の潜在バグ**: `DWMWA_EXTENDED_FRAME_BOUNDS` は物理ピクセルを返すため、DPI 非対応プロセスの仮想化座標と食い違い、スケーリング環境でパネルが画面外に出得る → **[ADR 0022](decisions/0022-ime-indicator-getwindowrect.md): `GetWindowRect` に統一**
3. オーバーレイ描画経路自体は正常: `ime_probe --overlay`（新設の視覚自己テスト）で SetWindowRgn / SetLayeredWindowAttributes / SetWindowPos 全て成功、`IsWindowVisible=true`、前面ウィンドウ中央の座標に配置されることを確認

### 対処後の再検証手段

- `cargo run --example ime_probe -- --overlay` — IME と無関係にパネル描画だけを目視確認
- `winremap --debug` — 照会のたびに `[debug] IME インジケーター: 状態=オン → パネル表示` 形式の診断行を出力（今回追加）
