# ADR 0002: Rust edition 2024・stable チャネルを採用する

- ステータス: 承認
- 日付: 2026-07-18

## 文脈

プロジェクト開始にあたり Rust の edition とツールチェーン方針を決める必要がある。開発環境は Rust 1.95（stable）。

## 決定

- **edition 2024** を採用する（Rust 1.85 以降で安定化済み）
- ツールチェーンは **stable チャネルのみ**。nightly 機能は使わない
- MSRV（最低サポートバージョン）は当面定めず、CI・開発とも最新 stable に追随する
- `rust-toolchain.toml` によるバージョン固定はしない（CI は `dtolnay/rust-toolchain@stable` を使用）

## 理由

- 新規プロジェクトで後方互換の制約がなく、最新 edition を選ばない理由がない。edition 2024 は unsafe 周りの明示化（`unsafe_op_in_unsafe_fn` の既定化等）が強化されており、unsafe を隔離する本プロジェクトの方針（ブリーフ §5-3）と相性が良い
- 配布形態が GitHub Releases の単一 exe であり、利用者が古いツールチェーンでビルドする需要が現状ない

## 却下した代替案

- **edition 2021**: 選ぶ積極的理由がない
- **MSRV の明示・固定**: crates.io 公開を検討する段階までは管理コストに見合わない。公開時に ADR で再判断する
- **nightly の利用**: 安定性最優先の方針（AGENTS.md）に反する
