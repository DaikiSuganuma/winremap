# AccessKit は遅延子ビューポートに付かない（調査記録）

- 作成日: 2026-07-26
- 作成: Claude Code（AI モデル: claude-opus-5）／判断: オーナー
- 対象: [v0.5 開発計画 §2](../01_development-plan.md)（Phase A・調査タスク）
- 関連: [ADR 0055](../decisions/0055-accesskit-for-child-viewports.md)、[ADR 0037（不可視ホスト＋子ビューポート）](../../v0.2/decisions/0037-gui-invisible-host-viewport.md)、[05_ui-test-automation.md](../../05_ui-test-automation.md)

計画 §2.2-1 の「時間を区切った調査」の結果。**読みは当たっていた**（feature を足すだけでは解決しない）ことを実測で確認した。設計判断は ADR 0055 が正。

## 何を確かめたか

`Cargo.toml` の eframe に `accesskit` feature を足したうえで、**WinRemap と同じビューポート構成**（不可視 1×1 ホスト＋遅延子ビューポート。ADR 0037）を最小再現し、UI Automation から見えるかを調べた。再現コードは [`examples/accesskit_probe.rs`](../../../examples/accesskit_probe.rs) に残してある。

```powershell
cargo run --example accesskit_probe          # ホスト＋遅延子ビューポート（WinRemap と同じ形）
cargo run --example accesskit_probe -- root  # 対照: 同じウィジェットをルートビューポートに描く
```

どちらも `ProbeHeading`（見出し）・`ProbeLabel`（ラベル）・`ProbeButton`（ボタン）の 3 つを描く。UIA 側は PowerShell の `System.Windows.Automation` で子孫を列挙した。

## 結果

| 構成 | UIA から見えた子孫 |
|---|---|
| **遅延子ビューポート**（WinRemap と同じ） | **0 個**。ウィンドウ自体は名前で見つかるが、中身は何も出ない |
| **ルートビューポート**（対照） | **9 個**。`ControlType.Text 'ProbeHeading'`、`ControlType.Text 'ProbeLabel'`、**`ControlType.Button 'ProbeButton'`** ＋ タイトルバー・システムメニュー等 |

同じ feature・同じ描画・同じウィジェットで、**違いはビューポートだけ**である。したがって:

- 「OpenGL で描いているから UIA に出ない」は**誤り**（[05_ui-test-automation.md](../../05_ui-test-automation.md) で訂正済み）。ルートなら同じ OpenGL 描画でも出る
- `accesskit` feature を有効にするだけでは WinRemap の設定・ログウィンドウは読めない。**有効化は必要だが十分ではない**

## 原因（コードの位置）

eframe 0.35 で `init_accesskit` を呼んでいるのは 2 箇所だけで、どちらも**ルートウィンドウ生成時**である。

- `eframe-0.35.0/src/native/glow_integration.rs:275-286` — `glutin.viewports.get_mut(&ViewportId::ROOT)` と**決め打ち**
- `eframe-0.35.0/src/native/wgpu_integration.rs:286-290` — 同じ位置づけ（本プロジェクトは glow を使う）

一方、子ビューポートのウィンドウと `egui_winit::State` は `glow_integration.rs` の別経路（`viewport.egui_winit.get_or_insert_with(...)`、1207 行付近）で作られ、そこには AccessKit の初期化が無い。

## 修正の見積もり

その `get_or_insert_with` の中で `State::init_accesskit(event_loop, window, proxy)` を呼べばよい、という形は素直である。ただし**イベントループのプロキシがその場に無い**: ルート側は `GlowWinitApp` の `self.repaint_proxy` から取っているが、子ビューポートを作るのは `GlutinWindowContext` 側で、そこまでプロキシを引き回す必要がある。**変更そのものは小さいが、引数の追加を伴う**。

## 上流の状況

- egui の最新リリースは **0.35.0（2026-06-25）** で、本プロジェクトが使っている版がすでに最新。**新しい版に上げれば直る、という話ではない**
- `emilk/egui` の issue を `accesskit` で検索した範囲では、子ビューポートに関する報告は見当たらなかった（検索は網羅的ではない）

## 未確認のまま残したこと

- **パッチが実際に効くか**は試していない。`[patch.crates-io]` でローカル改変版を指して、子ビューポートの子孫が UIA に出ることを確かめるところまでやれば、ADR 0055 の選択に迷いが無くなる
- wgpu バックエンドは確認していない（本プロジェクトは glow のみ）
