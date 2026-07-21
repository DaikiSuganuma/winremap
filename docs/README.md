# docs/ — ドキュメント構成

- 作成日: 2026-07-20（旧 `docs/decisions/README.md` を改組）
- 作成: Claude Code（AI モデル: claude-fable-5）／レビュー・承認: オーナー

## 構成ルール（オーナー決定 2026-07-20）

- **docs 直下**: アプリ全体の設計・バージョン非依存の規約と、本索引のみを置く
- **細かい仕様・開発計画・作業文書**: リリースバージョンのフォルダ（`v0.1/`、`v0.2/`、…）へ置く
- **decisions / notes**: 各バージョンフォルダの配下に置く。新バージョンのフォルダは、保存すべき文書ができた時点で作成する
- **番号プレフィックス**: 読むべき順序を示す。各バージョンフォルダ内では `01_` から振り直す（docs 直下の番号とは独立）
- 迷ったらオーナーに確認する

## docs 直下（アプリ全体）

| 文書 | 内容 |
|---|---|
| [01_project-brief.md](01_project-brief.md) | 開発経緯・要件・アーキテクチャ・不変条件（一次資料） |
| [02_rust-guidelines.md](02_rust-guidelines.md) | Rust 開発の作法（バージョン非依存） |
| [03_release-operations.md](03_release-operations.md) | リリース運用手順（オーナー向けランブック） |
| [04_git-branching.md](04_git-branching.md) | Git ブランチ運用（git-flow の適用。マージは常に `--no-ff`） |

## バージョン別

| フォルダ | 内容 |
|---|---|
| [v0.1/](v0.1/) | v0.1.0 の開発計画（01）、設定仕様（02）、受け入れチェックリスト（03）、アイコン（04）、IME インジケーター設計・計画（05・06）、decisions/（ADR 0001-0028）、notes/ |
| [v0.2/](v0.2/) | v0.2 開発計画（[01_development-plan.md](v0.2/01_development-plan.md)）、GUI フレームワーク比較調査（[02_gui-framework-study.md](v0.2/02_gui-framework-study.md)）、受け入れチェックリスト（[03_acceptance-checklist.md](v0.2/03_acceptance-checklist.md)）、設定 GUI 設計書（[04_config-gui-design.md](v0.2/04_config-gui-design.md)）、decisions/（ADR 0029-0042） |
| [v0.3/](v0.3/) | v0.3 開発計画（[01_development-plan.md](v0.3/01_development-plan.md)）。多言語ファイル化・マクロ記憶機能・winget/scoop 対応。ADR は 0043 から |

設定ファイルの利用者向け説明は、docs ではなく[ヘルプサイト](../site/)（GitHub Pages）で分かりやすく提供する運用とする（詳細仕様書 `v0.1/02_config-spec.md` は開発者向け）。

## ADR（Architecture Decision Record）の規約

ADR とは「なぜその設計にしたか」「却下した代替案は何か」を 1 判断 1 ファイルで残すドキュメント形式のこと。各バージョンフォルダの `decisions/` に置く。

- ファイル名: `NNNN-短い-スラッグ.md`（連番はバージョンをまたいで通し番号）
- 必須内容: ステータス / 日付 / 作成（AI モデル名）/ 文脈 / 決定 / 理由 / 却下した代替案
- 新しい設計判断（クレート追加、仕様変更、アルゴリズム選択）を行うたびに 1 件追加する（AGENTS.md ワークフロー 2）
- 過去の ADR は書き換えず、決定を覆す場合は新しい ADR で上書きし、旧 ADR のステータスを「superseded by NNNN」に変更する
