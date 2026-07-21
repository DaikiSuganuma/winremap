# ADR 0047: ヘルプサイトのフレームワークを Pico CSS から Bootstrap へ変更する

- ステータス: 承認（オーナー指示）
- 日付: 2026-07-22
- 作成: Claude Code（AI モデル: claude-opus-4-8）

## 文脈

[ADR 0046](0046-help-site-pico-css.md) でヘルプサイトに Pico CSS を採用し、4 ページ（2 言語 × 2 ページ）を Pico の意味的マークアップに寄せて実装した。

オーナーが実際の表示を確認したところ、**Pico 採用前の手書きデザインのほうが余白と配色が好みだった**との評価だった。前デザインは特定のフレームワークを使っておらず、手書き CSS 245 行に GitHub（Primer）系の配色をハードコードし、本文カラムを 860px 幅に絞ったものだった。Pico は `.container` が最大 1130px まで広がるため本文が横に伸び、既定のトーンも前デザインとは異なっていた。

これを受けてオーナーから「**Pico CSS をやめて Bootstrap に変更する**」との指示があった（2026-07-22）。あわせて、Bootstrap のダークモードの扱いを確認し、「**小さなインライン JS で OS のダーク/ライトに自動追従する**」方式を選択した（オーナー決定 2026-07-22）。

ADR 0046 を覆す変更なので新しい ADR として記録し、0046 のステータスを「superseded by 0047」に変更する（docs/README.md の ADR 規約）。

## 決定

1. **Bootstrap v5.3.3 を採用**し、jsDelivr から読み込む。ADR 0046 で決めた CDN 許可（jsDelivr）はそのまま踏襲する

2. **CSS のみを読み込む。** Bootstrap の JavaScript バンドル（`bootstrap.bundle.min.js`）は読み込まない。使うのはリセット・タイポグラフィ・テーブル・ボタン・コード表示・カラーモードだけで、いずれも CSS で足りる

3. **ダークモードは数行のインライン JS で OS 設定に自動追従する。** Bootstrap 5.3 のカラーモードは `<html data-bs-theme>` 属性で切り替える方式であり、OS の `prefers-color-scheme` に追従させるにはスクリプトが要る。各ページの `<head>` に、外部を一切読まない自己完結のインライン `<script>` を置き、`matchMedia('(prefers-color-scheme: dark)')` を見て `data-bs-theme` を `dark`/`light` に設定する（変更イベントにも追従）

4. **バージョンを固定し、SRI を付ける。** `@5.3.3` を指定し、`integrity="sha384-…"`（Bootstrap 公式が公開する値と一致することを確認済み）と `crossorigin="anonymous"` を併記する

   ```html
   <link rel="stylesheet"
         href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css"
         integrity="sha384-QWTKZyjpPEjISv5WaRU9OFeRpok6YctnYmDr5pNlyT2bRjXh0JMhjY6hW+ALEwIH"
         crossorigin="anonymous">
   ```

5. **`site/style.css` は Bootstrap の上に載る薄い層に保つ。** Bootstrap が意見を持たない部分（本文カラム幅、固定ヘッダー、ヒーロー、コールアウト 2 種、コードブロックの背景）だけを定義し、**色は Bootstrap のカスタムプロパティ（`--bs-*`）を参照する**。自前で配色とダークモードを二重に持たない

6. **オーナーの好みに合わせて 2 点だけ前デザインへ寄せる。** (a) `.container` の最大幅を 860px に絞る。(b) リンク色とプライマリボタンを前デザインの GitHub 系の青（ライト `#0969da`／ダーク `#4493f8`）に戻す。これは `--bs-link-color-rgb` と `.btn-primary` の `--bs-btn-*` の上書きだけで済む

7. **これはウェブサイトの話であり、アプリの不変条件とは無関係である。** `winremap.exe` は引き続きネットワーク通信を一切行わない。サイトに載せたインライン JS も外部を読まない（ブリーフ §10「ネットワーク通信を行うコードの追加禁止」は本体コードの話であり、変更しない）。ADR 0046 決定 6 と同じ立場を維持する

## 理由

- **オーナーの直接指示である。** 見た目の最終判断はオーナーが持つ。Pico と Bootstrap はどちらも同種の CSS フレームワークであり、載せ替えは HTML のクラス付けと薄い overlay の書き換えで完結する
- **前デザインの不満（余白・配色）に直接応える。** カラム幅 860px と GitHub 系の青を overlay で戻せば、「前のほうが好き」の中身をそのまま満たせる。フレームワークを捨てて手書きに戻すより、テーブル・コードブロック・ダークモードの維持を Bootstrap に任せられる点が優れている
- **ADR 0046 で Bootstrap を却下した理由は「重い／JS を伴う」だった。** 今回は CSS のみを読み込み、JS はダークモード追従の数行だけをインラインで持つ。JS バンドルを読まないので、当時の懸念（GitHub Pages への配置が単純なファイルコピーでなくなる、JS 由来の挙動が入る）は生じない
- **バージョン固定 + SRI が CDN 利用の作法である**（ADR 0046 と同じ）。配信側の変更でサイトが勝手に崩れることを防ぎ、改竄があればブラウザが適用を拒否する
- **落ちても読める。** CSS が来なければ素の意味的 HTML として表示され、インライン JS が動かなければライトテーマで表示される。どちらも文章・コード例・表はすべて読める

## 却下した代替案

- **Pico CSS のまま使う（ADR 0046 を維持）**: 外部依存は同じ 1 つで済む。しかしオーナーが前デザインの余白・配色を好み、Bootstrap への変更を明示的に指示したため却下する

- **フレームワークをやめて手書き CSS に戻す**: 前デザインそのものに戻せて外部依存がゼロになる。しかしオーナーの指示は「Bootstrap に変更」であり、手書きへの回帰ではない。ページが増えるほどテーブル・コードブロックの体裁を 2 言語 4 ページ分そろえる手間が増える点も、フレームワークを使う動機のままである

- **Bootstrap の JS バンドルも読み込む**: ナビの折りたたみ（ハンバーガー）やドロップダウンが使えるようになる。しかしこのサイトのナビは単純な横並びで足り、FAQ はネイティブの `<details>` で済む。JS を増やす必要がなく、「JS が来なくても壊れない」性質を保つために読み込まない

- **ダークモードをライト固定にして JS をゼロにする**: 最も単純だが、OS がダークのときも白背景になる。前デザイン・Pico 版ともに OS 追従だったので、体験を後退させないためインライン JS で追従させる（オーナー決定）

- **CSS のメディアクエリだけでダークモードを自作する**: JS ゼロを保てるが、Bootstrap の `data-bs-theme` 方式と二重管理になり、`--bs-*` を `prefers-color-scheme` で上書きし続ける保守が発生する。数行のインライン JS のほうが安い

- **可動タグ `@5`／`@5.3` を使う**: 更新の手間は減るが、SRI を付けられず、配信側の変更がそのまま見た目に出る（ADR 0046 と同じ理由で却下）

## 付記: ライセンス表示

Bootstrap は MIT ライセンスである。ADR 0046 と同じ方針で、**`THIRD-PARTY-NOTICES.md` への追加は行わない**。同ファイルは「`winremap.exe` に同梱されるもの」を対象としており、Bootstrap のコピーを我々が配布することはないためである。代わりに**ヘルプサイトのフッターにクレジットを置く**（Pico のクレジットは Bootstrap のものへ差し替えた）。

## 参照（公式）

- Bootstrap: https://getbootstrap.com/
- Bootstrap カラーモード: https://getbootstrap.com/docs/5.3/customize/color-modes/
- Bootstrap CDN と SRI: https://getbootstrap.com/docs/5.3/getting-started/download/#cdn-via-jsdelivr
- Subresource Integrity (MDN): https://developer.mozilla.org/en-US/docs/Web/Security/Subresource_Integrity
