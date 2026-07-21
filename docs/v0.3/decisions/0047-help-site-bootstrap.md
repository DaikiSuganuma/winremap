# ADR 0047: ヘルプサイトのフレームワークを Pico CSS から Bootstrap へ変更する

- ステータス: 承認（オーナー指示）
- 日付: 2026-07-22
- 作成: Claude Code（AI モデル: claude-opus-4-8）

## 文脈

[ADR 0046](0046-help-site-pico-css.md) でヘルプサイトに Pico CSS を採用し、4 ページ（2 言語 × 2 ページ）を Pico の意味的マークアップに寄せて実装した。

オーナーが実際の表示を確認したところ、**Pico 採用前の手書きデザインのほうが余白と配色が好みだった**との評価だった。前デザインは特定のフレームワークを使っておらず、手書き CSS 245 行に GitHub（Primer）系の配色をハードコードし、本文カラムを 860px 幅に絞ったものだった。

これを受けてオーナーから「**Pico CSS をやめて Bootstrap に変更する**」との指示があった（2026-07-22）。続けて同日、さらに次の方針が示された。

- **Bootstrap のコンポーネントをフル活用**して見やすく書き換える
- **アイコンは Bootstrap Icons を使う**
- **カラム幅を含め、Bootstrap がデフォルトで提供するものを使う**（前デザインへ寄せる上書きはしない）
- **`style.css` の記述量は最小化する。** 理想は「色（WinRemap アイコンの色に合わせる）と `.hero` だけ」
- **バージョンは最新の 5.3.8** を使う

ADR 0046 を覆す変更なので新しい ADR として記録し、0046 のステータスを「superseded by 0047」に変更する（docs/README.md の ADR 規約）。

## 決定

1. **Bootstrap v5.3.8 を採用**し、jsDelivr から読み込む。ADR 0046 で決めた CDN 許可（jsDelivr）はそのまま踏襲する

2. **Bootstrap のコンポーネントをそのまま使う。** ナビは navbar（レスポンシブに折りたたむトグラー付き）、FAQ はアコーディオン、注記は alert、表は `table table-bordered table-sm`（ヘッダーは `<thead>`）、ボタンは `btn`、任意機能の見出しには badge、といった具合に、独自スタイルを避けて既製部品で組む

3. **Bootstrap Icons を採用する**（v1.11.3、jsDelivr、固定 + SRI）。ボタンや各セクション見出し、alert のアイコンに使う

4. **JavaScript バンドル（`bootstrap.bundle.min.js`）を読み込む。** navbar の折りたたみとアコーディオンに必要なため。**これは ADR 初版（CSS のみ）からの変更点**である。バンドルは Popper を含む単一ファイルで、jsDelivr から固定 + SRI で読み込む

5. **ダークモードは数行のインライン JS で OS 設定に自動追従する。** Bootstrap 5.3 のカラーモードは `<html data-bs-theme>` 属性で切り替える方式であり、OS の `prefers-color-scheme` に追従させるにはスクリプトが要る。各ページの `<head>` に、外部を一切読まない自己完結のインライン `<script>` を置く

6. **バージョンを固定し、SRI を付ける。** CSS・JS・アイコン CSS のいずれも `@5.3.8` / `@1.11.3` を指定し、`integrity="sha384-…"`（Bootstrap 公式が公開する値と一致することを確認済み）と `crossorigin="anonymous"` を併記する

   ```html
   <link rel="stylesheet"
         href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/css/bootstrap.min.css"
         integrity="sha384-sRIl4kxILFvY47J16cr9ZwB07vP4J8+LH7qKQnuqkuIAvNWLzeN8tE5YBujZqJLB"
         crossorigin="anonymous">
   ```

7. **`site/style.css` は「色」と「`.hero`」だけに削る**（オーナーの理想）。中身は 2 つ。(a) WinRemap アイコンの青（`#0078D4`）を Bootstrap の primary・リンク色（`--bs-primary` / `--bs-link-color-rgb` / `.btn-primary`）に両テーマ分だけ割り当てる。(b) ヒーローの帯の背景。**カラム幅などレイアウトは Bootstrap の既定に任せ、上書きしない**（前デザインの 860px 上書きは撤回）

8. **これはウェブサイトの話であり、アプリの不変条件とは無関係である。** `winremap.exe` は引き続きネットワーク通信を一切行わない。サイトに載せた JS（Bootstrap バンドルとインラインのテーマ切り替え）も外部を読まない（ブリーフ §10「ネットワーク通信を行うコードの追加禁止」は本体コードの話であり、変更しない）。ADR 0046 決定 6 と同じ立場を維持する

## 理由

- **オーナーの直接指示である。** 見た目の最終判断はオーナーが持つ。Pico と Bootstrap はどちらも同種の CSS フレームワークであり、載せ替えは HTML のクラス付けと薄い overlay の書き換えで完結する
- **コンポーネントで組むと `style.css` が最小化できる。** navbar・アコーディオン・alert・table・btn・badge をそのまま使えば、レイアウトやコールアウトを自前 CSS で持つ必要がなくなる。結果として `style.css` は「色 + `.hero`」に収まり、オーナーの理想に一致する
- **アイコンの色をブランド色に一致させられる。** WinRemap のアイコンは `#0078D4`。これを Bootstrap の primary に割り当てると、ボタン・リンク・見出しアイコンが本体アイコンと同じ青でそろう
- **ADR 0046 で Bootstrap を却下した理由は「重い／JS を伴う」だった。** 今回はオーナーがコンポーネントのフル活用を明示的に望んでおり、navbar の折りたたみとアコーディオンのために JS バンドルを読み込む。GitHub Pages への配置は依然として静的ファイルのコピーのみで完結する（ビルド手順は増えない）
- **バージョン固定 + SRI が CDN 利用の作法である**（ADR 0046 と同じ）。配信側の変更でサイトが勝手に崩れることを防ぎ、改竄があればブラウザが適用を拒否する
- **落ちても読める。** CSS が来なければ素の意味的 HTML として表示され、JS が動かなくてもアコーディオンは各項目が開いた状態で全文が読め、テーマはライトになる。文章・コード例・表はすべて読める

## 却下した代替案

- **Pico CSS のまま使う（ADR 0046 を維持）**: 外部依存は 1 つで済み、JS も要らない。しかしオーナーが前デザインの余白・配色を好み、Bootstrap への変更とコンポーネントのフル活用を明示的に指示したため却下する

- **CSS のみ（JS バンドルを読まない）で通す**: 「JS が来なくても壊れない」性質を最大化できる。しかし navbar の折りたたみやアコーディオンといったコンポーネントが使えず、「コンポーネントをフル活用」という指示に反する。JS が無い状況でも内容は全て読めるため、バンドルを読む不利益は小さい

- **前デザインへ寄せる上書き（カラム 860px・GitHub 青）を続ける**: 最初の不満（余白）には効くが、オーナーが「Bootstrap の既定を使う」と明示したため撤回する。ブランド色（アイコンの青）への一致だけは色の指定として残す

- **アイコンを絵文字や自前 SVG で用意する**: 外部依存を増やさずに済む。しかしオーナーが Bootstrap Icons を指定しており、Bootstrap と同じ CDN・作法（固定 + SRI）で一貫させられる

- **可動タグ `@5`／`@5.3` を使う**: 更新の手間は減るが、SRI を付けられず、配信側の変更がそのまま見た目に出る（ADR 0046 と同じ理由で却下）

## 付記: ライセンス表示

Bootstrap・Bootstrap Icons はいずれも MIT ライセンスである。ADR 0046 と同じ方針で、**`THIRD-PARTY-NOTICES.md` への追加は行わない**。同ファイルは「`winremap.exe` に同梱されるもの」を対象としており、これらのコピーを我々が配布することはないためである（トレイアイコンに使う Bootstrap Icons の**ラスタライズ済み画素**は従来どおり同ファイルに記載がある）。代わりに**ヘルプサイトのフッターにクレジットを置く**。

## 参照（公式）

- Bootstrap: https://getbootstrap.com/
- Bootstrap カラーモード: https://getbootstrap.com/docs/5.3/customize/color-modes/
- Bootstrap CDN と SRI: https://getbootstrap.com/docs/5.3/getting-started/download/#cdn-via-jsdelivr
- Bootstrap Icons: https://icons.getbootstrap.com/
- Subresource Integrity (MDN): https://developer.mozilla.org/en-US/docs/Web/Security/Subresource_Integrity
