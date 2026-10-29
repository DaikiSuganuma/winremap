# ADR 0083: 不変条件のうち機械で確かめられるものはテストにし、unsafe の許可リストは 1 つのファイルを正本にする

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- ステータス: **承認（オーナー決定 2026-10-29「unsafe 許可リストの正本を承認」、[v1.1 開発計画](../01_development-plan.md) §8-3・§8-4）**
- 関連: [ADR 0009](../../v0.1/decisions/0009-bootstrap-unsafe-placement.md)（unsafe を置く場所の最初の決定）、[ADR 0020](../../v0.1/decisions/0020-ime-indicator-scope.md)・[0031](../../v0.2/decisions/0031-notify-module-unsafe-allowlist.md)・[0038](../../v0.2/decisions/0038-gui-win32-module.md)・[0041](../../v0.2/decisions/0041-local-time-for-session-banners.md)・[0044](../../v0.3/decisions/0044-macro-replay-thread.md)・[0058](../../v0.5/decisions/0058-log-readability.md)・[0061](../../v0.6/decisions/0061-packaged-config-path.md)・[0063](../../v0.7/decisions/0063-symbol-keys.md)・[0067](../../v0.8/decisions/0067-ime-cursor-color.md)・[0080](../../v1.0.1/decisions/0080-tray-icon-asks-for-the-small-metric.md)・[0081](../../v1.0.1/decisions/0081-icon-must-not-depend-on-its-background.md)（許可リストにファイルを足した ADR）

## 背景

AGENTS.md の不変条件 3（unsafe の隔離）は、**どのファイルに `unsafe` を書いてよいか**と、**各ブロックに `// SAFETY:` を付けること**を定める。v0.1 の 3 ファイルから始まり、v1.0.1 までに 11 本の ADR が 1 ファイルずつ足して、13 ファイル＋検証用の example 1 本になった。

この不変条件は**文章とレビューだけで守られていた**。違反しても `cargo test` も clippy も赤くならない。しかも文章の正本が 3 つある。

| 場所 | 内容 |
|---|---|
| AGENTS.md 不変条件 3 | 13 ファイル＋`examples/ime_probe.rs`。**実態と一致**している |
| ブリーフ §5-3 | 11 ファイル。`cursor.rs`（ADR 0067）と `gui/win32.rs` のトレイ分（ADR 0080・0081）の追記が無い |
| guidelines §3 | 「`hook.rs` / `sender.rs` / `window.rs` の 3 ファイルのみ」。v0.1 のまま |

ブリーフは「矛盾があればブリーフが正」と言うが、実際に新しいのは AGENTS.md の方で、**正本関係が逆転している**。1 ファイル足すたびに 3 か所を直す運用は、2 回続けて守られなかった。

v1.1 はコードの置き場所を大きく動かす版である（[開発計画](../01_development-plan.md)）。`hook.rs` や `cursor.rs` をディレクトリ化し、スレッドメッセージの配管を 1 ファイルに集めるとき、**unsafe がどこへ移ったかを文章で追いかけるのは、この版こそ無理がある**。

## 決定

### 決定 1 — 許可リストの正本は `tests/unsafe-allowlist.txt` の 1 ファイル

1 行 1 ファイル。各行に、そのファイルを許可した ADR を `#` コメントで併記する。AGENTS.md 不変条件 3・ブリーフ §5-3・guidelines §3 は、**同じ 1 文でこのファイルを参照する**だけにし、ファイル名を列挙しない。

ファイルを足す・外すときは、**ADR を書き、同じコミットでこの一覧を直す**。

### 決定 2 — `tests/invariants.rs` が `cargo test` で検査する

3 つの検査を結合テストとして置く。

1. **`unsafe` の実使用があるファイルの集合 ＝ 許可リストの集合。** 両方向で見る — 許可外のファイルに `unsafe` があれば赤、許可リストにあるのに `unsafe` が無くなったファイルがあっても赤（一覧が実態から離れて「意味のない名簿」になるのを防ぐ）。存在しないパスも赤
2. **すべての `unsafe {` の直上に `// SAFETY:` がある。** 判定は「`unsafe {` の行から上へ、コメント行は `SAFETY:` を探しながら読み、属性行は飛ばし、空行で打ち切り、コードの行は 1 行だけ跨いでよい」。1 行の猶予は rustfmt が `let x =` の次の行へ `unsafe {` を折り返す場合のためで、2026-10-29 時点の 125 ブロックすべてがこの規則を満たすことを測ってから決めた
3. **ライブラリクレート（`src/lib.rs` が宣言するモジュール）は `windows::` / `cfg(windows)` / `cfg(target_os` を含まない。** ブリーフ §9「純粋ロジックとしてヘッドレス CI で回す」の裏付け

検査はソースを**テキストとして**読む。コメント（`//` 以降）は外してから照合するので、`unsafe` について書いた文章は違反にならない。

### 決定 3 — clippy の `undocumented_unsafe_blocks` は使わない

同じことを lint でも検査できるが、`Cargo.toml` の `[lints]` に置くことは guidelines §1「rustfmt / clippy とも設定ファイルによるカスタマイズはしない」に当たる。決定 2 の検査 2 が同じ約束を守るので、規約を変えてまで lint を足さない（オーナー決定 2026-10-29、開発計画 §8-4）。

## 理由

- **動かす前に網を張る。** v1.1 の移動は十数ブランチに分かれる。どの 1 つで `unsafe` が許可外へ漏れても、あるいは SAFETY コメントが移動の途中で落ちても、CI が止める
- **正本が 1 つなら、ずれようがない。** 3 か所の文章は「このファイルを見よ」になり、ファイル自体はテストが実態と突き合わせる
- **ADR の併記で「なぜこのファイルか」が一覧から引ける。** 不変条件 3 の段落に詰まっていた 11 本の ADR 参照を、一覧の各行へ移した
- **テキスト走査で足りる。** `syn` 等のパーサを dev-dependency に足せば正確にはなるが、規則は行の形で決まっていて、依存を増やす理由（guidelines §8）に届かない

## 却下した代替案

- **文章のまま、レビューで守る** — これまでの運用。2 回続けて 3 か所の同期が落ちた
- **正本を AGENTS.md の段落にし、テストが AGENTS.md を読む** — Markdown の段落を機械が読む形にすると、段落が人に読みにくくなる。一覧は一覧の形が読みやすい
- **`#![deny(unsafe_code)]` を unsafe を含まないモジュールに付ける** — Rust 標準の仕組みで、許可外の `unsafe` はコンパイルエラーになる。ただし「どのファイルに付けたか」がまた文章になり、`mod.rs` に付けると子モジュールにも効くので `ime_indicator/mod.rs` のような「親は unsafe なし、子はあり」の形で使えない。将来、許可リストの検査と併用する形で足すことは妨げない
- **clippy `undocumented_unsafe_blocks` を `Cargo.toml` の `[lints]` で有効化** — 決定 3 のとおり

## 影響・補足

- 受け入れ R-3（[03_acceptance-checklist.md](../03_acceptance-checklist.md)）: 許可外に `unsafe` を 1 つ書いて赤になることを 1 度確かめる
- `examples/ime_probe.rs` は一覧に載っているが、v1.1 の M5（開発計画 G1）で削除する予定。削除のコミットで一覧からも外す
- 検査 3 は `src/lib.rs` の `pub mod` 行を読んでライブラリの範囲を決める。モジュールを足せば自動で対象になる
