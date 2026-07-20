# ADR 0028: GitHub Pages によるヘルプサイト（手書き静的 HTML・Actions デプロイ）

- ステータス: 承認（オーナー指示）
- 日付: 2026-07-20
- 作成: Claude Code（AI モデル: claude-fable-5）
- 参照: [GitHub Pages 公開ソースの設定](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site) / [actions/configure-pages](https://github.com/actions/configure-pages) / [actions/upload-pages-artifact](https://github.com/actions/upload-pages-artifact) / [actions/deploy-pages](https://github.com/actions/deploy-pages)

## 文脈

v0.1.0 公開に合わせ、オーナーから「GitHub Pages を利用した分かりやすいヘルプページ」の作成指示。README は機能列挙型で開発者寄りのため、エンドユーザーの作業順（インストール → 最初の設定 → リファレンス → 困ったとき）に沿ったヘルプを別途用意する。構成は 1 ページ完結、言語は英語（正）+ 日本語（追随）とすることをオーナーが承認済み。

## 決定

**`site/` ディレクトリに手書きの静的 HTML/CSS を置き、GitHub Actions（`configure-pages` → `upload-pages-artifact` → `deploy-pages`）で GitHub Pages にデプロイする。**

1. 構成: `site/index.html`（英語・正）、`site/ja/index.html`（日本語・追随）、`site/style.css`、`site/assets/`（リポジトリ `assets/` からコピーしたアイコン）
2. ビルドツール・外部 CDN・Web フォント・JavaScript を使わない完全自己完結の静的サイトとする
3. デプロイは `.github/workflows/pages.yml`（main への push で `site/**` が変わったときと手動実行）。ブランチではなく Actions アーティファクト方式（GitHub 推奨の現行方式）
4. 公開 URL: `https://daikisuganuma.github.io/winremap/`

## 理由

- **1 ページのヘルプにビルドチェーンは過剰**: 生成系を挟むと依存とビルド失敗という新しい故障点が増える。手書き HTML なら PR の diff がそのまま最終成果物で、レビューも容易
- **自己完結 = 製品の信頼性の打ち出しと整合**: 本体は「ネットワークコード無し」を掲げている。ヘルプサイトも外部 CDN・トラッカー・フォント配信への依存をゼロにし、一貫した姿勢を保つ
- **Actions アーティファクト方式**は gh-pages ブランチが不要で、公開内容が main の `site/` と常に一致する（ブランチ保護・CODEOWNERS の統制下に置ける）

## 却下した代替案

- **Jekyll テーマ（Pages 組み込みビルド）**: テーマ・Liquid・Ruby 依存が増え、生成結果の細部制御が面倒。1 ページには利点がない
- **`/docs` フォルダを公開ソースにする**: `docs/` は日本語の開発ドキュメント（ブリーフ・ADR 等）専用であり、エンドユーザー向けヘルプと混在させると双方の目的を損なう
- **`gh-pages` ブランチ方式**: main 外に状態を持ち、ブランチ保護の対象管理が複雑になる。旧方式であり新規採用の理由がない
- **mdBook / MkDocs 等のドキュメントジェネレーター**: 多ページの本格ドキュメントには適するが、1 ページ完結の方針に対して過剰。必要になったら別 ADR で再判断する
