# ADR 0087: 問いに答え終えた調査プローブは `examples/` から消す

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- ステータス: **承認（[v1.1 開発計画](../01_development-plan.md) G1、オーナー決定 2026-10-29「推奨案で承認」）**
- 関連: [ADR 0020](../../v0.1/decisions/0020-ime-indicator-scope.md)（`ime_probe.rs` を置いた決定）、[ADR 0030](../../v0.2/decisions/0030-config-gui-framework-egui.md)・[ADR 0034](../../v0.2/decisions/0034-ime-preedit-underline-visuals.md)（`egui_ime_probe.rs` が答えた問い）、[ADR 0055](../../v0.5/decisions/0055-accesskit-for-child-viewports.md)・[v0.5 ノート](../../v0.5/notes/20260726_accesskit-child-viewport.md)（`accesskit_probe.rs` が答えた問い）、[ADR 0083](0083-invariants-as-tests.md)（許可リスト）

## 背景

`examples/` には性格の違うものが 3 種類あった。

| 種類 | ファイル | 役目 |
|---|---|---|
| 道具 | `app_icons.rs`、`msix_assets.rs` | アイコンと Store 資産を SVG から焼く。[06_icon-assets.md](../../06_icon-assets.md) §4 から呼ぶ |
| 調査プローブ | `ime_probe.rs`（v0.1）、`egui_ime_probe.rs`（v0.2）、`accesskit_probe.rs`（v0.5） | 設計判断の前に「できるか」を確かめた |
| サンプル設定 | `*.toml`、`registry/` | 利用者向け。結合テストの入力でもある |

プローブ 3 本は、それぞれの問いに答え終えている。`ime_probe.rs` は IME の状態を読める方式を決め（ADR 0020、`detect.rs` に実装された）、`egui_ime_probe.rs` は egui で日本語入力ができることを確かめ（ADR 0030・0034）、`accesskit_probe.rs` は子ビューポートに AccessKit が付かないことを示した（ADR 0055、フォークで解決済み）。

残しておく費用は小さくない。`cargo clippy --all-targets` と `cargo test` が毎回ビルドし、`ime_probe.rs` は unsafe 許可リストに 1 行を占め、`egui_ime_probe.rs` は `src/gui` のフォント読み込みを写した複製を持っていた（「本体がどう変わっても動くように」と書かれたとおり、本体と離れていく）。

## 決定

3 本を削除する。履歴には残る（`git show v1.0.1:examples/ime_probe.rs`）ので、当時の検証コードを読みたい ADR は、そのタグ付きのパスで参照する。

`tests/unsafe-allowlist.txt` から `examples/ime_probe.rs` を外す。`detect.rs` の「`ime_probe.rs` が回帰確認として残る」の注記は、実際には回帰確認として回されたことが無い（受け入れは実機の IME で行う）ので、その文ごと消す。

道具 2 本とサンプル設定は残す。

## 理由

- **問いが閉じたプローブは、残しても測らない。** 3 本とも、答えを出した版の受け入れ以後に実行された記録が無い
- **写しは本体から離れる。** `egui_ime_probe.rs` のフォント読み込みは `gui/mod.rs` の v0.2 時点の写しで、もう同じではない
- **許可リストが短くなる。** unsafe を含む `examples/` のファイルが無くなり、検査対象（ADR 0083）も `src/` と `examples/` の道具だけになる

## 却下した代替案

- **`examples/probes/` に移して残す** — Cargo の example は `examples/` 直下か `[[example]]` の明示が要る。明示してまで残す理由が無い
- **`#[cfg(feature = "probes")]` で普段はビルドしない** — ビルドされないコードは腐る。履歴から取り出す方が、動いた時点の形が保証される

## 影響・補足

- 過去の ADR・ノートの `examples/ime_probe.rs` 等への言及はそのまま（当時の事実）。読む側は `git show <tag>:examples/<name>.rs` で取り出す
- `docs/06_icon-assets.md` §3 の表から `egui_ime_probe.rs` の行を消す
