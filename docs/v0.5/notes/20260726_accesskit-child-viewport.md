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

## パッチの実測（ADR 0055 決定 4・同日実施）

eframe 0.35.0 のソースをローカルに複製して改変し、`[patch.crates-io]` で一時的に指して測った（改変版は作業用のため未コミット。内容は下記）。

**結果: 効く。**

| 測定 | 結果 |
|---|---|
| 子ビューポートの子孫 | **10 個**。`Text 'ProbeHeading'`・`Text 'ProbeLabel'`・**`Button 'ProbeButton'`**・動的ラベル（`clicks seen by the host: 0`）＋ タイトルバー等 |
| ボタンの操作 | `InvokePattern` が露出しており、UIA から `Invoke()` を呼ぶと**プローブ側に `ProbeButton clicked` が出た** |

読めるだけでなく**押せる**ことまで確認できた。Phase A が必要としていたのはまさにこれである。

### 改変の中身（2 箇所）

1. `GlutinWindowContext` に `accesskit_proxy: Option<EventLoopProxy<UserEvent>>` を持たせ、`initialize_window` で**ウィンドウを持つ全ビューポート**にアダプターを作る（`ViewportId::ROOT` 決め打ちをやめる）。`ActiveEventLoop` にはプロキシ生成が無いため、フィールドで持ち回る必要がある
2. **ウィンドウを一旦隠したまま作る。** これは実測して初めて分かった制約で、最初のパッチはこう落ちた:

   > `The AccessKit winit adapter must be created before the window is shown (made visible) for the first time.`（`accesskit_winit-0.32.2/src/lib.rs:198`）

   ルートは白いちらつき対策（[egui #2279](https://github.com/emilk/egui/pull/2279)）で元から `visible(false)` で作られるため踏まなかった。子ビューポートは可視で作られるので踏む。アダプターを付けてから `set_visible(true)` する形にした

### 上流 PR にする前に要ること

- **wgpu バックエンドにも同じ変更**（`wgpu_integration.rs`。本プロジェクトは使わないが、片方だけ直すのは PR として不完全）
- egui のコントリビューション手順（CHANGELOG への記載、CI）に合わせる
- `#[cfg_attr(not(feature = "accesskit"), expect(unused_mut))]` のような細工を、上流の書き方に寄せて整理する

## WinRemap 本体での確認（同日実施）

プローブは最小再現にすぎないため、**本体の設定ウィンドウ・ログウィンドウ**を VM 上で実測した。採取は [`tests/ui/guest/dump-uia.ps1`](../../../tests/ui/guest/dump-uia.ps1)（`.\run-vm-ui-test.ps1 -DumpUia`）で行い、**AI を介在させていない**。

**結果: 本体でも効いている。**

| ウィンドウ | 子孫 | 中身 |
|---|---|---|
| `WinRemap — settings`（表示モード・全体設定） | **43** | `Button 'Edit'`、`ComboBox` (value=`minimal.toml`)、`Text 'C:\Test'`、ナビの `General`／`Keymaps`／`notepad`、`Text 'WinRemap v0.5.0'` |
| 同（`notepad` 選択後） | **59** | 上記＋ `Text 'notepad.exe'`・`Text 'C-h'`・`Text 'Back'`・コメント行 |
| 同（`Edit` を UIA から押した後） | **72** | ヘッダーが `Button 'Revert'`＋`Button 'Save'` に変わり、`Edit` 要素が値付きで出る（`notepad` / `notepad.exe` / `C-h` / `Back`）。`Add application`・`Capture the foreground app`・`Target all applications`・`Add rule` も名前付きボタンとして見える |
| `WinRemap — log` | **7** | `CheckBox 'Follow newest'`(Toggle=On)、`Button 'Clear'`、`Button 'Copy all'`、ログ行がそれぞれ `Text` |

**押せることも本体で確認した。** `Button 'Edit'` を `InvokePattern` で叩くと編集モードに入り、ツリーが 59 → 72 に変わった。読み取りだけでなく操作が届いている。

v0.4 で編集機能に入れた要素（アプリ追加・前面アプリ取得・全アプリ対象・規則追加）がすべて名前付きで出ているため、**Phase B の回帰自動化はこのまま機械判定に載せられる**見込みが立った。

### 分かった制約: トレイメニューは UIA に出ない

トレイの右クリックメニューは Win32 のポップアップ（クラス `#32768`）で、**この UIA クライアントからは中身のない `Pane` にしか見えない**（項目が 1 つもツリーに出ない）。AccessKit とは無関係で、Windows のメニューの性質である。採取スクリプトは Windows 本来の操作（頭文字で選択 → Enter）で開いている。

シナリオ側は terminator MCP がメニュー項目を扱えているため、ここは影響しない。

## 未確認のまま残したこと

- wgpu バックエンドは確認していない（本プロジェクトは glow のみ）
