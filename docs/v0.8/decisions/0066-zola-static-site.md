# ADR 0066: ヘルプサイトを Zola で生成し、生成物をリポジトリにコミットする

- 作成日: 2026-08-02
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）
- ステータス: 採用（**オーナー決定 2026-08-02**「静的サイトジェネレーターは Zola にします。生成物をコミットしたいです」）
- 関連: [ADR 0028](../../v0.1/decisions/0028-github-pages-help-site.md)（ヘルプサイトを GitHub Pages で出す決定）、[`.github/workflows/pages.yml`](../../../.github/workflows/pages.yml)（`site/` をそのまま配信している）、[v0.8 開発計画 §2](../01_development-plan.md)
- 公式: [Zola](https://www.getzola.org/documentation/getting-started/overview/)（[Multilingual sites](https://www.getzola.org/documentation/content/multilingual/)・[`zola build`](https://www.getzola.org/documentation/getting-started/cli-usage/)）／[GitHub Pages: Publishing with a custom workflow](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)

## 背景

ヘルプサイトは `site/*.html`（英語）と `site/ja/*.html`（日本語）に**手書きの HTML が 2 本ずつ**ある。ナビゲーション・フッター・`<head>`・Bootstrap の読み込みが**全ページに複製**されており、1 か所の変更が最大 14 ファイルに波及する。

v0.7 の実績で言うと、`faq.html` を 2 本新設し、`config.html` を日英とも大幅に書き換え、リリース当日にも FAQ へ同じ段落を 2 回足した。**書いた内容は 1 つで、作業は毎回 2 回**である。翻訳の追随漏れは、その 2 回のあいだで起きる。

## 決定

1. **Zola を採用する。** Rust 製の単一バイナリで、テンプレート（Tera）と多言語が本体機能である。**Node のツールチェーンをこのリポジトリに持ち込まない**
2. **生成物をリポジトリにコミットする**（オーナー決定）。原稿は `site-src/`（Zola プロジェクト。`config.toml`・`content/`・`templates/`・`static/`）、**出力先は現在の `site/`**
3. **`pages.yml` は変更しない。** 現行ワークフローは `site/` をそのまま Pages に上げているので、出力先を `site/` にすれば**配信経路に一切手を入れずに済む**
4. **見た目は変えない。** 現在の Bootstrap ベースの体裁をテンプレートへ移植する。v0.6 で撮った Store 用スクリーンショットを撮り直さずに済ませるためである
5. **生成物と原稿のずれを CI で検出する。** `ci.yml` に `zola build` を実行して `git diff --exit-code -- site` を見るジョブを足す。**コミットし忘れた生成物は赤になる**

参照（公式）:

- Zola のインストールと `zola build`: https://www.getzola.org/documentation/getting-started/cli-usage/
- 多言語（`config.toml` の `languages` と `content/*.<lang>.md`）: https://www.getzola.org/documentation/content/multilingual/

## 理由

**生成物をコミットする形は、このプロジェクトでは素直に働く。** 一般には「ビルド成果物を版管理に入れない」が原則だが、ここでは次の 3 つが効く。

- **配信経路が無変更で済む。** Pages のワークフローに Zola のインストールを足さなくてよく、**サイトが壊れる経路が 1 本増えない**
- **公開されている HTML が履歴に残る。** 「あのとき何が書いてあったか」をリポジトリだけで追える。ヘルプサイトは利用者への約束（設定の書き方・制限事項）を載せる場所なので、これは実利がある
- **手元で確認できる。** Zola を入れていない環境でも `site/` を開けば現物が見える

代償は**差分が二重になる**ことである（原稿 1 か所の修正が、生成物では複数ファイルに出る）。レビューは原稿側を見るものとし、生成物側は決定 5 の CI が「原稿と一致していること」だけを保証する。

## 却下した代替案

- **Hugo**: 同じく単一バイナリで多言語も強い。Zola と実力差は小さいが、Rust のプロジェクトで Rust 製のツールを選べるなら、**手を入れる必要が生じたときに読める**ほうを採る
- **Astro / Eleventy**: 表現力は上だが **Node と `node_modules` を持ち込む**。常駐キーリマッパーのリポジトリに、サイトのためだけに依存ツリーとその監査を抱える理由がない
- **生成物をコミットせず CI で生成する**: 一般論としてはこちらが正道だが、オーナーが生成物のコミットを選択した。`pages.yml` に Zola のインストールを足す構成は、必要になれば決定 2 を覆すだけで移行できる
- **手書きのまま続ける**: 二重管理が続く。v0.7 で 1 つの段落を 2 回書いた作業は、このまま増える

## 影響・補足

- **`site/` は生成物になる。** 直接編集してはならない。`site/` を手で直すと次の `zola build` で消える。`.gitattributes` に `site/** linguist-generated=true` を入れて、GitHub の差分表示で畳む
- **原稿の言語対応**: 日本語が原稿の第一言語であり、英語版が対訳である（README と同じ関係）。Zola の多言語機能で 1 つのページに `content/faq.md` と `content/faq.en.md` を並べる形にする
- **移行の完了条件**: 生成された `site/` が、移行前の `site/` と**表示上同じであること**。文言の変更は移行と混ぜない（混ぜると差分が読めなくなる）
