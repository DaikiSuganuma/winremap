# WinRemap 開発計画（v0.2）

> 元資料: [01_project-brief.md](../01_project-brief.md) §3.2（v0.2 候補）、[v0.1 開発計画](../v0.1/01_development-plan.md) Phase 5。
> Rust 実装の作法は [03_rust-guidelines.md](../03_rust-guidelines.md) を参照。

- 作成日: 2026-07-20
- 作成: Claude Code（AI モデル: claude-fable-5）／レビュー・承認: オーナー

---

## 1. スコープ（オーナー決定: 2026-07-20）

### 採用

1. **設定 GUI（Graphical User Interface）** — v0.2 の目玉機能
2. **起動時コンソールウィンドウの非表示化** — 現在は console subsystem のため、ダブルクリック起動でログ出力用のコンソールウィンドウが表示される。これをやめ、ログ出力方式を再設計する
3. **winget / scoop 対応** — パッケージマネージャー経由でのインストールを可能にする

### 見送り（ブリーフ §3.2 の残候補。理由を記録）

| 候補 | 見送り理由 |
|---|---|
| ワンショットモディファイア / tap-hold | Windows 標準の固定キー機能（Sticky Keys）で代替可能。「OS 標準に任せる」方針に従う |
| マークモード | 当面の需要なし |
| ウィンドウタイトルによる切り替え | 当面の需要なし |
| 設定ファイル監視による自動リロード（ADR 0008 で v0.2 再検討としていた件） | トレイメニューからのリロードで十分。依存とデバウンスの複雑さに見合わない |
| expand-region（選択範囲の段階的拡大、[検討ノート](../v0.1/notes/20260719_expand-region-study.md)） | 不変条件の例外リスト改訂が必要になる規模に対し需要が小さい |

方針はブリーフどおり **1 機能 1 ADR（Architecture Decision Record: 設計判断とその理由の記録）** で、各フェーズ着手前にオーナー承認を得る。

---

## 2. Phase A — コンソール非表示化とログ出力方式

v0.1 は console subsystem でビルドされ、起動・リロード・`--debug` の全メッセージを `println!` / `eprintln!` で出力している。ウィンドウを出さないためには `#![windows_subsystem = "windows"]` への変更が必要で、その場合 stdout/stderr は既定でどこにも接続されないため、ログの受け皿を決める必要がある。

### ログ出力方式の選択肢（比較の記録。決定は [ADR 0029](decisions/0029-attach-console-and-tray-log-window.md)）

| 方式 | 概要 | 長所 | 短所 |
|---|---|---|---|
| A. `AttachConsole(ATTACH_PARENT_PROCESS)` | ターミナルから起動されたときだけ親コンソールに接続して出力 | ダブルクリックは無音、`--debug`・`--help` は従来どおりターミナルで見える。追加ウィンドウなし | プロンプト表示と出力が混ざる（cmd/PowerShell の仕様）。エクスプローラー起動時はログが完全に消える |
| B. `--debug` 時のみ `AllocConsole` | デバッグ時だけ専用コンソールを生成 | デバッグ体験が v0.1 と同等 | 通常起動時のエラー（設定エラー等）の通知先が別途必要 |
| C. ログファイル出力 | `%APPDATA%\winremap\` 等にファイル書き込み | 事後調査が可能 | **不変条件 6（キーロガー化の禁止）に抵触しうる**。`--debug` はキー名レベルのログを含むため、ディスク永続化は既定 OFF でも慎重な設計が必要 |
| D. `OutputDebugStringW` | デバッガー向け出力（DebugView 等で閲覧） | 実装が最小、ウィンドウ不要 | 一般利用者には閲覧手段がなく実質開発者専用 |
| E. Windows イベントログ | OS のイベントログへ記録 | OS 標準の閲覧 UI がある | 常駐ツールのキー名ログを残す先として過剰かつ不適。登録も煩雑 |
| F. 設定 GUI 内のログビュー | Phase B の GUI にログ表示ペインを設ける | 将来的に一番使いやすい | GUI 完成が前提。v0.2 内では順序の制約が生じる |

**オーナー決定（2026-07-20、[ADR 0029](decisions/0029-attach-console-and-tray-log-window.md)）**: 次の 2 本立てとする。

1. **A（AttachConsole）**: ターミナルから起動されたときだけ、そのターミナルに出力する。ダブルクリック等での起動は無音
2. **トレイの「ログ表示」ウィンドウ**: タスクトレイを右クリックして「ログ表示」を選択すると専用ウィンドウが開き、デバッグモードのログをそのウィンドウに出力する（F の前倒し・独立実装に相当）

ファイル出力（C）は不変条件 6 との整合を考え採用しない。B / D / E も却下（理由は ADR 0029）。

参照（公式）:

- windows_subsystem 属性: https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute
- AttachConsole: https://learn.microsoft.com/en-us/windows/console/attachconsole
- AllocConsole: https://learn.microsoft.com/en-us/windows/console/allocconsole
- OutputDebugStringW: https://learn.microsoft.com/en-us/windows/win32/api/debugapi/nf-debugapi-outputdebugstringw

### タスク

1. ~~ADR 0029 — ログ出力方式の選定~~（起案済み: [ADR 0029](decisions/0029-attach-console-and-tray-log-window.md)）
2. `#![windows_subsystem = "windows"]` 化と `AttachConsole(ATTACH_PARENT_PROCESS)` の実装
   - 注意: `AttachConsole` 等の unsafe 呼び出しの配置は不変条件 3 の unsafe 隔離リスト（hook.rs / sender.rs / window.rs / ime_indicator）に含まれない。ADR 0009（bootstrap unsafe の配置）の改訂または新 ADR での明示的な例外追加が必要（**オーナー承認事項**）
3. トレイメニュー「ログ表示」— 選択でログウィンドウを開き、開いている間はデバッグモードのログをそのウィンドウへ出力する。ログはフックから既存のロックフリーキュー（ADR 0016 の機構）経由で受け取り、フックコールバック内に処理を追加しない。ウィンドウを閉じたときの挙動（デバッグモード解除）も設計する
4. `--help` / `--version` / 設定エラー時の動作確認と通知先の再設計（無音起動時のエラーはメッセージボックスまたはトレイバルーンで通知）
5. インストーラー（Inno Setup）・ショートカット経由の起動確認
6. v0.2 用の受け入れチェックリストを `docs/v0.2/` に作成し（v0.1 の[受け入れチェックリスト](../v0.1/03_acceptance-checklist.md)を基に）、確認項目を追加

### 完了条件

- ダブルクリック / スタートメニュー / スタートアップ起動でコンソールウィンドウが出ない
- ターミナルから `--debug` / `--help` を実行すると v0.1 と同等の情報が得られる
- トレイの「ログ表示」でウィンドウが開き、デバッグモードのログがリアルタイムに確認できる。表示中もリマップが停止・遅延しない
- 設定エラーが無音で握りつぶされない（利用者に見える形で通知される）

---

## 3. Phase B — 設定 GUI（目玉機能）

### タスク

1. ADR 0030 — GUI 技術選定。候補比較（いずれも MIT/Apache-2.0 系であること、ネットワーク通信を行わないこと):
   - egui/eframe（即時モード、単一 exe に同梱しやすい）
   - Slint（宣言的 UI、royalty-free ライセンスの確認要）
   - iced（Elm 風）
   - ネイティブ Win32（依存ゼロだが開発コスト大）
   - Tauri（WebView ベース。WebView2 依存とフットプリントの評価要）
   - 別プロセスにするか本体組み込みにするか（フック安定性への影響が判断軸。**安定性 > 単純さ > 機能** の優先順位に従う）
2. `docs/v0.2/02_config-gui-design.md` — 画面設計・編集モデルの設計書
   - 対象範囲: キーマップの閲覧・編集・保存・リロード連携、バリデーションエラーの行番号表示、`[ime_indicator]` 等トップレベル設定の編集
   - TOML との往復編集（コメント保持の可否）の方針
3. 実装（マイルストーン分割は設計書で確定。目安: 閲覧 → 編集・保存 → トレイ/リロード統合）
4. テスト — GUI 層とロジック層を分離し、編集モデルは純粋ロジックとしてテスト（フック層のテスト免除ルールと同様、GUI 描画層は手動確認）
5. README（en/ja）・ヘルプサイト（`site/`）への反映

### 完了条件

- GUI から設定の閲覧・編集・保存ができ、保存後のリロードで即反映される
- 不正な設定値は保存前に行番号つきで指摘される
- GUI を開いている間もリマップが停止・遅延しない（不変条件 2 の遵守）

---

## 4. Phase C — winget / scoop 対応

配布物は既存の GitHub Releases（installer + portable exe + SHA256SUMS + attestation）をそのまま参照する。アプリ本体へのコード変更は原則不要（ネットワーク通信禁止の不変条件にも抵触しない）。

### タスク

1. ADR 0031 — 配布チャネル方針（winget / scoop それぞれのマニフェスト提出先、バージョン更新の運用、公式チャネルとしての位置づけ）
2. winget: `microsoft/winget-pkgs` へのマニフェスト提出（installer は Inno Setup 製のため `installerType: inno`）。リリースごとの更新自動化（winget-releaser 等の GitHub Actions）を評価
3. scoop: 提出先の選定 — 公式 Extras バケットへの提出、または自前バケット（`suganuma/scoop-winremap` 等）。portable exe + `autoupdate` 定義
4. README / ヘルプサイトのインストール手順に winget / scoop を追記。「他サイト配布バイナリは非公式」の記述と整合させる（マニフェストは公式 Releases の URL を参照するため公式チャネルと明記できる）
5. `docs/06_release-operations.md`（リリース運用手順）にパッケージ更新手順を追記

参照（公式）:

- winget パッケージ提出: https://learn.microsoft.com/en-us/windows/package-manager/package/
- Scoop App Manifests: https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests

### 完了条件

- `winget install winremap`（正式 ID は提出時に確定）と `scoop install winremap` でインストールでき、インストール後の起動・アンインストールが正常
- リリース手順書にバージョン更新フローが記録されている

---

## 5. Phase D — リリース（v0.2.0）

1. `CHANGELOG.md` 0.2.0 化（Unreleased のヘルプサイト分を含む）
2. v0.2 用受け入れチェックリスト（`docs/v0.2/`、Phase A タスク 6 で作成）で手動受け入れテスト実施・記録
3. タグ `v0.2.0` → GitHub Release（installer + portable + SHA256SUMS + attestation）
4. winget / scoop マニフェストの更新提出（Phase C の手順に従う）

---

## 6. フェーズ順序と依存関係

```
Phase A（コンソール非表示） → Phase B（設定 GUI） → Phase D（リリース）
                                Phase C（winget/scoop）─┘
```

- Phase A を先行させる（規模が小さく、GUI のエラー通知設計が A の結論に依存するため）
- Phase C は本体コードと独立しており、Phase B と並行可能。ただし winget/scoop への初回提出は v0.2.0 リリース後でもよい（その場合 v0.1.0 の資産で先行提出する選択肢もある — オーナー判断）

## 7. オーナー判断が必要な項目

| フェーズ | 判断事項 |
|---|---|
| A | ~~ログ出力方式の採用案~~（2026-07-20 決定済み: [ADR 0029](decisions/0029-attach-console-and-tray-log-window.md)）、unsafe 隔離リストの例外追加 |
| B | GUI フレームワークの選定承認（ライセンス確認込み）、GUI の機能範囲 |
| C | scoop の提出先（Extras か自前バケットか）、winget/scoop 提出を v0.2.0 前に行うか後に行うか |
| D | v0.2.0 リリース判断 |
