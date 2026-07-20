# 設定 GUI フレームワーク比較調査（gpui-component / egui / Slint）

> v0.2 の目玉機能である設定 GUI（[01_development-plan.md](01_development-plan.md) Phase B）のフレームワーク選定のための調査。
> オーナーが **gpui-component** を検討中であることを受け、egui・Slint との違いを調べた。
> 本書は調査資料であり決定ではない。決定は ADR 0030 で行う。

- 作成日: 2026-07-20
- 作成: Claude Code（AI モデル: claude-fable-5）／レビュー・承認: オーナー
- 調査時点: 2026-07-20。バージョン・スター数等は変動するため、判断時に再確認すること

## 0. 前提条件（WinRemap 側の要件）

| 要件 | 由来 |
|---|---|
| ライセンスは MIT / Apache-2.0 系が望ましい | 本体が MIT。AGENTS.md 禁止事項（LGPL の kanata コード不可）と同じ慎重さ |
| 日本語 UI 必須 | ADR 0014（日英 UI ローカライズ） |
| 単一 exe 配布を維持 | ブリーフ §3.1。外部ランタイム（WebView2 等）依存は避けたい |
| ネットワーク通信をしない | AGENTS.md 禁止事項 |
| タスクトレイ常駐と共存 | 既存トレイは `tray-icon` クレート（ADR 0007）。ウィンドウは必要なときだけ開く |
| キーフックのメッセージループと共存 | 不変条件 1・2。GUI がフックを止めない・遅延させないこと（優先順位: 安定性 > 単純さ > 機能） |

## 1. 総合比較

| 観点 | gpui-component | egui | Slint |
|---|---|---|---|
| 正体 | Zed の GPU UI フレームワーク **gpui** 上のコンポーネント集（60+ ウィジェット）。開発元 Longbridge | 即時モード GUI ライブラリ本体（作者 emilk） | 宣言的 UI（`.slint` DSL）ツールキット（SixtyFPS GmbH） |
| ライセンス | Apache-2.0 | **MIT OR Apache-2.0** | GPL-3.0-only **OR** Royalty-free 2.0（帰属表示義務）**OR** 商用（有償） |
| 最新版（調査時点） | gpui-component 0.5.1 / gpui 0.2.2 | 0.35.0（2026-06-25） | 1.17.1（2026-07-07、**1.x 安定版**） |
| 配布 | crates.io にあるが**公式は git 依存を指示** | crates.io で完結 | crates.io で完結 |
| GitHub スター | 12.1k | 29.8k | 23.3k |
| 描画方式 | 保持モード寄り / GPU（Windows は DirectX 11 + DirectWrite） | **即時モード** / GPU（glow, wgpu） | 保持モード / GPU（Skia, FemtoVG）+ **ソフトウェアレンダラ** |
| 日本語表示 | 対応（DirectWrite のシステムフォント） | **既定フォントに CJK 非同梱。自前でフォント読込が必須** | システムフォント自動選択 |
| 日本語 IME | gpui に実装あり。ただし Zed Windows で不具合報告あり | 0.29〜0.35 で段階的に改善中。Windows 実績は**不明** | winit backend が preedit/commit を処理 |
| バイナリサイズ | 約 12MB | **約 5MB** | 3〜6.5MB（コミュニティ報告、非公式） |
| 外部ランタイム | 不要 | 不要 | 不要（Qt backend を選ばない限り） |
| トレイ常駐との相性 | **不明**（事例が見つからない） | `tray-icon` に**公式 egui サンプルあり** | `run_event_loop_until_quit()` が公式にトレイ用途を想定 |

## 2. 観点別の詳細

### 2.1 正体

- **gpui-component**: Zed エディタの GPU アクセラレーテッド UI フレームワーク `gpui` の上に構築されたコンポーネントライブラリ。開発元は Longbridge（証券会社）で、同社の Longbridge Pro が実運用第 1 号。macOS/Windows のコントロールと shadcn/ui のデザインに影響を受けた 60+ コンポーネント、Tree-sitter シンタックスハイライト、Markdown/HTML 表示、仮想化リスト、チャートを含む
  - https://github.com/longbridge/gpui-component / https://longbridge.github.io/gpui-component/
- **egui**: 「シンプル・高速・高移植性の即時モード GUI ライブラリ」。`eframe` がデスクトップ/Web 用ラッパー
  - https://github.com/emilk/egui
- **Slint**: 組込み・デスクトップ・モバイル向けの宣言的 GUI ツールキット。`.slint` マークアップで UI を書き、ロジックは Rust 等
  - https://slint.dev/ / https://github.com/slint-ui/slint

### 2.2 ライセンス（最重要）

- **egui**: `MIT OR Apache-2.0`。商用利用・再配布とも制約なく、帰属表示義務もない。**WinRemap の要件に完全合致**
  - https://crates.io/crates/egui
- **gpui / gpui-component**: crates.io メタデータ・README とも **Apache-2.0**。商用利用可。ただし git 依存で Zed 本体リポジトリから取り込む場合は、当該リポジトリが複数ライセンス構成のため実地確認が必要（未検証）
  - https://crates.io/crates/gpui / https://crates.io/crates/gpui-component
- **Slint**: `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` のトリプルライセンス
  - **GPLv3**: 無償だが配布時にアプリのソース公開義務。WinRemap は OSS なので理論上は可能だが、**本体の MIT から GPLv3 への実質的なライセンス変更**を意味する
  - **Royalty-free 2.0**: プロプライエタリでも無償利用可だが**帰属表示義務**あり（トップレベルメニューから到達できる About 画面に `AboutSlint` ウィジェットを表示、またはバイナリ配布ページに Slint バッジを掲示）。組込みでの利用不可
  - **商用**: 有償サブスクリプション
  - → **MIT/Apache 系という要件からは外れる**
  - https://crates.io/crates/slint / https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md / https://slint.dev/pricing

### 2.3 成熟度・安定性

- **egui**: 0.35.0（2026-06-25）。2〜3 か月ごとにマイナーリリース。累計約 1,998 万 DL。実運用例に Rerun Viewer。**1.0 未到達で、リリースごとに破壊的変更がある**
  - https://github.com/emilk/egui/blob/master/CHANGELOG.md
- **Slint**: **1.x 安定版**（1.17.1、2026-07-07）。3 者中で唯一セマンティックバージョニング上の 1.0 保証がある。累計 135 万 DL
- **gpui-component**: crates.io に 0.5.1 はあるが、**公式 Getting Started は git 依存を指示**しており実質 git 追従が前提。gpui の crates.io 版 0.2.2 は古い（2025-10-22）。**バージョン固定・再現ビルドの観点で最も不利**。リポジトリ自体は活発
  ```toml
  gpui = { git = "https://github.com/zed-industries/zed" }
  gpui-component = { git = "https://github.com/longbridge/gpui-component" }
  ```
  - https://longbridge.github.io/gpui-component/docs/getting-started

### 2.4 Windows サポートと日本語

- **egui**: eframe が Windows を含む主要プラットフォームをサポート。**日本語表示は最大の弱点**で、既定フォントに CJK グリフを同梱しておらず、自前で `FontDefinitions` にフォントを登録する必要がある（要望 Issue #3060 は closed as not planned）。定型コード数十行で解決できるが、フォント埋め込みならバイナリが数 MB 増える。IME は 0.29 で `Event::Ime` 追加、0.35 で composition の視覚表現を実装と改善が続くが、**Windows 日本語 IME の実運用実績は確認できず不明**
  - https://github.com/emilk/egui/issues/3060 / https://github.com/emilk/egui/blob/master/CHANGELOG.md
- **Slint**: フォントは実行システムから自動選択され、`default-font-family` で指定可。カスタムフォント埋め込みも可。IME は winit backend が `Ime::Preedit` / `Ime::Commit` を処理
  - https://docs.slint.dev/latest/docs/slint/guide/development/fonts/
- **gpui**: Zed の Windows 版は 2025 年 10 月に stable 到達。DirectX 11 + DirectWrite。ただし日本語 IME 関連の既知不具合が実際に報告されている（Google 日本語入力の英数キーで IME 切替不可 = Issue #40638、PR #41259 で修正済み／ATOK の入力切替キー不具合 = Issue #40321）。**WinRemap は IME インジケーターを持つ日本語重視のアプリであり、この点は要注意**
  - https://zed.dev/docs/windows / https://github.com/zed-industries/zed/issues/40638 / https://github.com/zed-industries/zed/issues/40321

### 2.5 単一 exe への同梱

3 者とも WebView2 等の外部ランタイム依存はない（Tauri / Electron に対する大きな利点）。サイズは gpui-component 自身の比較表で gpui-component **12MB** / iced 11MB / egui **5MB**。Slint は公式数値が見つからず、コミュニティ報告で 3〜6.5MB 程度（一次情報ではない）。

- https://github.com/longbridge/gpui-component / https://github.com/slint-ui/slint/discussions/9570

### 2.6 トレイ常駐との相性（WinRemap 固有の最重要観点）

- **egui**: `tray-icon` クレートに**公式の egui サンプル**（`examples/egui.rs`）が存在し、`eframe::run_native` のクロージャ内で `TrayIconBuilder` を構築して同一イベントループを共有する。既存資産（ADR 0007 で採用した tray-icon）との整合性は 3 者中で最良。ただし **eframe は既存のイベントループ上では動かせない**（Issue #2875）ため、「ウィンドウを閉じてトレイに格納 → 再度開く」を厳密に行うには `egui-winit` + `egui-wgpu` で自前の winit ループを持つ必要がある
  - https://github.com/tauri-apps/tray-icon/blob/dev/examples/egui.rs / https://github.com/emilk/egui/issues/2875
- **Slint**: **`run_event_loop_until_quit()`** が公式に用意され、「最後のウィンドウが閉じてもイベントループを継続し `quit_event_loop()` まで動く」＝システムトレイアプリ向けと明記されている。ウィンドウは `hide()` / `show()` で出し入れ。タスクバー非表示は標準では不可で winit の Window を取得する必要あり
  - https://releases.slint.dev/1.4.0/docs/rust/slint/fn.run_event_loop_until_quit
- **gpui-component**: `Application::run()` がメインスレッドを取る設計。`Application::headless()` は存在するがウィンドウを開けなくなるものでトレイ用途とは別。**tray-icon との共存実績・作法は確認できず不明**。gpui は Windows で独自のメッセージループを持つため、**既存のキーフック用メッセージループとの共存も要検証**（不変条件 1・2 に直結する最大のリスク）
  - https://docs.rs/gpui/latest/gpui/struct.Application.html

## 3. 用途別の評価

### egui — 第一候補（日本語フォントの手当てが必須）

- ライセンス（MIT OR Apache-2.0）が要件に完全合致、帰属表示義務なし
- `tray-icon` に公式サンプルがあり、既存のトレイ実装をそのまま活かせる
- バイナリ最小クラス（約 5MB）、crates.io で完結しバージョン固定が容易
- 設定 GUI のような「たまに開く単純なフォーム」は即時モードと相性が良い（状態同期のコードが不要）
- ✗ 日本語フォントを自前で埋め込む／読み込む必要がある（定型コードだが必須）
- ✗ Windows 日本語 IME の実績が不明。テキスト入力欄が少なければ影響は限定的だが、早期に実機検証すべき
- △ 1.0 未到達で毎リリース破壊的変更あり

### Slint — 技術的には最良だがライセンスが要件に合わない

- 唯一の 1.x 安定版。日本語フォントがシステムから自動で効き、IME も実装済み
- `run_event_loop_until_quit()` が明示的にトレイ常駐アプリを想定しており、この用途に最も素直
- ソフトウェアレンダラがあり GPU なし環境でも動く（常駐アプリとして堅い）
- ✗ MIT/Apache 系ではない。無償利用は GPLv3（本体を MIT から実質変更）か Royalty-free 2.0（About 画面等での帰属表示義務）
- ✗ `.slint` DSL の学習コストとビルドステップ（build.rs）が増える

### gpui-component — 現時点では非推奨

- ライセンス Apache-2.0、コンポーネントが豊富で見た目の完成度は高い
- ✗ **公式が git 依存を指示**しており、crates.io での安定バージョン固定ができない。長期メンテする常駐ユーティリティには不向き
- ✗ バイナリ約 12MB と 3 者中最大
- ✗ **トレイ常駐との共存作法・実績が不明**。`Application::run` が主導権を取る設計で、キーフックのメッセージループとの共存も未検証。不変条件 1・2（フックを止めない）に対するリスクが読めない
- ✗ Windows 対応は 2025 年 10 月に stable 化したばかりで、日本語 IME の個別不具合が実際に報告されている
- → 設定ダイアログ程度の UI に対して、依存の重さとリスクが釣り合わない

## 4. 調査者の推奨（決定は ADR 0030 でオーナーが行う）

**egui（eframe または egui-winit）を第一候補**とし、次の 2 点をプロトタイプで早期に潰すことを推奨する。

1. 日本語フォントの手当て（Noto Sans JP の埋め込み、または `C:\Windows\Fonts\meiryo.ttc` 等のシステムフォント読込）
2. Windows 日本語 IME での日本語入力（設定名などの入力欄を設ける場合）

gpui-component は見た目の魅力は大きいものの、**git 依存によるバージョン固定不能**と**トレイ／フックのメッセージループ共存が未知**という 2 点が、「安定性 > 単純さ > 機能」の優先順位に照らして不利。採用する場合は、設定 GUI の実装前に「トレイ常駐 + キーフック稼働中に gpui ウィンドウを開閉できるか」を検証する小さなプロトタイプ（`examples/` 配下）を必須とすべき。

Slint は技術的完成度が高く、ライセンス方針として GPLv3 または帰属表示を受け入れられるなら有力な選択肢となる。
