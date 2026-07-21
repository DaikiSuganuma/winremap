# ADR 0046: ヘルプサイトに Pico CSS を採用する（CDN 経由）

- ステータス: superseded by [0047](0047-help-site-bootstrap.md)（2026-07-22、オーナー指示で Bootstrap へ変更）
- 日付: 2026-07-21
- 作成: Claude Code（AI モデル: claude-opus-4-8）

> **注記（2026-07-22）**: 本 ADR で採用した Pico CSS は、オーナーの好み（前デザインの余白・配色に近づけたい）により Bootstrap へ差し替えた。判断の経緯は [ADR 0047](0047-help-site-bootstrap.md) を参照。以下は当時の記録として残す。

## 文脈

ヘルプサイト（[ADR 0028](../../v0.1/decisions/0028-github-pages-help-site.md)）は開設以来、**外部アセットを一切持たない**方針で作ってきた。`site/style.css` は手書きの 245 行で、配色・ダークモード・タイポグラフィ・テーブル・コールアウトをすべて自前で定義している。

v0.3 で設定ガイド（`config.html`）を追加し、ページ数が 2 言語 × 2 ページになった。表・コードブロック・定義的な一覧が大幅に増え、手書き CSS の維持コストが上がっている。オーナーから「Pico CSS を使って整える。**jsDelivr CDN を許可する**」との指示があった（2026-07-21）。

これは**サイトが初めて外部依存を持つ**という変更なので、判断として記録する。

## 決定

1. **Pico CSS v2.1.1 を採用**し、jsDelivr から読み込む

2. **バージョンを固定する。** `@2` のような可動タグではなく `@2.1.1` を指定する

3. **SRI（Subresource Integrity）を付ける。** `integrity="sha384-…"` と `crossorigin="anonymous"` を併記し、配信されたファイルが想定と 1 バイトでも違えばブラウザが適用を拒否するようにする

   ```html
   <link rel="stylesheet"
         href="https://cdn.jsdelivr.net/npm/@picocss/pico@2.1.1/css/pico.min.css"
         integrity="sha384-L1dWfspMTHU/ApYnFiMz2QID/PlP1xCW9visvBdbEkOLkSSWsP6ZJWhPw6apiXxU"
         crossorigin="anonymous">
   ```

4. **`site/style.css` は Pico の上に載る薄い層に書き換える。** Pico が持たない要素（ヒーロー、コールアウト 2 種、固定ヘッダー）だけを定義し、**色は Pico のカスタムプロパティ（`--pico-*`）を参照する**。自前で配色とダークモードを二重に持たない

5. **HTML は Pico の想定する意味的マークアップに寄せる。** ナビゲーションは `<nav><ul>…</ul></nav>`、ボタン風リンクは `role="button"` とする

6. **これはウェブサイトの話であり、アプリの不変条件とは無関係である。** `winremap.exe` は引き続きネットワーク通信を一切行わない（ブリーフ §10「ネットワーク通信を行うコードの追加禁止」は変更しない）。混同されないよう本 ADR に明記する

## 理由

- **Pico はクラスレス志向**である。既に書いてある `<section>` `<table>` `<pre>` `<kbd>` `<details>` がそのまま整う。クラス名を大量に振る種類のフレームワーク（Tailwind 等）と違い、**既存の HTML をほとんど書き換えずに済む**
- **ダークモードが付いてくる。** 現在は `prefers-color-scheme` のメディアクエリで 15 個の変数を二重に定義している。Pico は `<meta name="color-scheme" content="light dark">` だけで両テーマを持つ
- **維持するものが減る。** 手書き CSS 245 行のうち、残ったのは Pico が意見を持たない部分だけになった
- **バージョン固定 + SRI が CDN 利用の作法である。** 可動タグ（`@2`）だと、配信側の更新でサイトの見た目が予告なく変わりうるうえ、SRI が付けられない。固定すれば「上げるのは自分が上げたとき」になる
- **落ちても読める。** CSS が来なくてもページは意味的 HTML のまま表示される。文章・コード例・表はすべて読める（見た目が素になるだけ）。JavaScript ではないので、機能が壊れる余地がない

## 却下した代替案

- **`pico.min.css` を `site/` に同梱する（ベンダリング）**: 訪問者のブラウザが第三者へ一切リクエストしなくなり、外部依存がゼロのまま整う。**技術的にはこれが最も筋が良い**。却下したのはオーナーが CDN を明示的に許可したためであり、方針を戻す場合はファイルを 1 つ置いて `href` を差し替えるだけで済む（本 ADR を上書きする ADR を書く）。なおその場合は更新の追随を自分で行う必要がある

- **可動タグ `@2` を使う**: 常に最新の 2.x が来るので更新の手間が無い。しかし SRI を付けられず、配信側の変更がそのままサイトの見た目に出る。ドキュメントサイトが勝手に崩れる可能性を受け入れる理由がない

- **手書き CSS のまま整える**: 外部依存ゼロを維持できるが、ページが増えるほど自前で持つ量が増える。テーブルとコードブロックの体裁を 2 言語 4 ページ分そろえる作業は、まさにフレームワークが解いている問題である

- **Bootstrap / Tailwind**: どちらもこの規模のサイトには重い。Tailwind はビルド手順（Node.js）が増え、GitHub Pages への配置が単なる静的ファイルのコピーでなくなる。Bootstrap は JavaScript を伴う

- **Pico の「クラスレス版」（`pico.classless.min.css`）を使う**: `.container` や `.secondary` が無くなるので、ヒーローのボタン並びと固定ヘッダーを自前で書く量が増える。通常版でも我々が使うクラスは 3 つ（`container` / `secondary` / `button` ロール）だけである

## 付記: ライセンス表示

Pico CSS は MIT ライセンスである。**`THIRD-PARTY-NOTICES.md` への追加は行わない**。同ファイルは「`winremap.exe` に同梱されるもの」を対象としており（Bootstrap Icons はラスタライズされた画素がバイナリに入る）、Pico のコピーを我々が配布することはないためである。代わりに**ヘルプサイトのフッターにクレジットを置く**。

## 参照（公式）

- Pico CSS: https://picocss.com/
- Pico CSS ドキュメント: https://picocss.com/docs
- Subresource Integrity (MDN): https://developer.mozilla.org/en-US/docs/Web/Security/Subresource_Integrity
- jsDelivr: https://www.jsdelivr.com/
