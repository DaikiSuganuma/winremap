# ADR 0072: IME の状態が「不明」のとき、カーソルの色は動かさない

- 作成日: 2026-08-04
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- ステータス: 採用（2026-08-04 の受け入れ（[v0.8 §8](../03_acceptance-checklist.md) M-1）で見つかった症状に対する手当て。[ADR 0067](0067-ime-cursor-color.md) が導入したカーソルの色替えの、状態が取れなかった場合の扱いを決めていなかった部分である）
- 関連: [ADR 0067](0067-ime-cursor-color.md)（IME をマウスカーソルの色で示す）、[ADR 0021](../../v0.1/decisions/0021-ime-indicator-trigger-keys.md)（状態を取り直す契機は前面変化とトグル候補キー）、[ADR 0023](../../v0.1/decisions/0023-ime-indicator-query-target.md)・[ADR 0033](../../v0.2/decisions/0033-ime-status-across-input-threads.md)（どのウィンドウに聞くか）、[ADR 0070](0070-agent-led-acceptance.md)（この症状を見つけた受け入れの形）
- 公式: [`SendMessageTimeout`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendmessagetimeoutw)（`SMTO_ABORTIFHUNG` は相手スレッドが応答していなければ待たずに失敗する）／[`WM_IME_CONTROL`](https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-control)（`IMC_GETOPENSTATUS`）

## 背景

IME の状態は 3 値である。`detect::query_foreground()` は `Option<bool>` を返し、`None` は**「オフ」ではなく「分からなかった」**と明記してある（`detect.rs:137`「`None` on a failed or timed-out query, which the caller treats as "unknown" rather than "off"」）。問い合わせは `SendMessageTimeoutW(WM_IME_CONTROL, IMC_GETOPENSTATUS)` で、`SMTO_ABORTIFHUNG` ＋ 100 ms の上限が付いている — **相手スレッドがメッセージを汲んでいなければ、待たずに失敗する**。

ところが**呼び出し側がその区別を捨てていた**:

```rust
let is_on = sample.open == Some(true);   // 不明 → false
if settings.change_cursor_color {
    crate::cursor::apply(is_on, settings.cursor_color);   // 色を外す
}
```

パネルにとってはこれで正しい。「分からないなら出さない」は設計書 §3.2 の判断であり、**誤って光るパネルは、出ないパネルより悪い**。だがカーソルは点滅ではなく**状態灯**である。色を外すことは「IME はオフだ」と**主張する**ことであり、答えの返らなかった問い合わせはその主張を支えない。

**非対称なのが効く。** 古い色は次の問い合わせで直る。**外した色は直らない** — 取り直す契機は前面変化とトグル候補キーだけ（[ADR 0021](../../v0.1/decisions/0021-ime-indicator-trigger-keys.md)）なので、前面が変わらないかぎり、**利用者が自分で IME を打ち直すまで戻らない**。

2026-08-04 の受け入れ M-1 でオーナーが見た症状がこれと同じ形をしている。メモ帳で変換中にマウスカーソルを窓の外へ出し、変換を確定して戻すと、**IME はオンなのに色が付いていない。IME をオフ・オンし直すと戻る。**

## 決定

**`sample.open` が `Some` のときだけ `cursor::apply` を呼ぶ。** `None` のときは何もしない — 直前の色がそのまま残る。

```rust
if settings.change_cursor_color
    && let Some(open) = sample.open
{
    crate::cursor::apply(open, settings.cursor_color);
}
```

パネル側（`shown` の計算）は**変えない**。3 値の扱いが 2 つで違うのは、**片方が点滅で片方が状態灯だから**である。

`detect.rs` が既に「呼び出し側は unknown を off として扱わない」と書いていたので、これは**新しい方針ではなく、書いてある方針にコードを合わせる修正**である。

## 却下した代替案

- **不明のときに問い合わせを再試行する**（短いタイマーで数回）: 症状には効くが、**何回・どの間隔で諦めるか**という判断が増え、応答しない相手（[ADR 0071](0071-debug-console-must-not-kill-the-app.md) が示したように、相手はいくらでも固まりうる）に対して定期的な問い合わせを続けることになる。まず「捨てない」ことだけを直し、それでも足りないと分かってから足す
- **不明のときは色を外し、代わりにマウス移動でも取り直す**: 取り直しの契機を増やすのは ADR 0021 の判断を覆すことであり、マウス移動は毎秒何十回も来る。**フックを止めない**という最優先事項に対して割が悪い
- **パネルも同じ扱いにする**（不明のときは今の表示を保つ）: パネルは「オンになった」瞬間に光るものなので、保つべき状態を持たない。設計書 §3.2 の判断は今も正しい
- **何もしない（利用者が IME を打ち直せば戻る）**: 状態灯が黙って嘘をつく状態を許すことになる。しかも**嘘をついていると気付く手がかりが無い** — 色が付いていないことは正常な状態と見分けが付かない

## 影響・補足

- **昇格したウィンドウの上では、直前の色が残るようになる。** UIPI で問い合わせが届かないので `None` になるためである（[v0.8 M-3](../03_acceptance-checklist.md)）。M-3 の通過条件「色は変わらず、パネルも出ない」は満たす — **変わらない**のだから — が、**直前が「オン」だった場合は色が付いたまま昇格した窓へ移る**。ADR 0067 決定 4 の読み（色が残っている ＝ IME がオン、または WinRemap が死んだ）は保たれる。**次の受け入れで M-3 をこの観点で見直すこと**
- **M-1 の症状がこれで直ったとは、まだ言えない。** 修正後に再現を試みたが、その回は**ログに「不明」が 1 行も出なかった**（4 分間）。**この修正は、それ自体が直すべき筋の通らない箇所を直したものであり、オーナーが見た症状の原因だと確かめたわけではない。** M-1 は次の受け入れで再確認するまで**不合格のまま**にしてある
- **`SMTO_ABORTIFHUNG` は変換中に効きやすい。** 相手スレッドが IME の変換処理でメッセージを汲んでいない間は、待たずに失敗する。オーナーの手順（**変換中**にマウスを窓の外へ出す）がこの経路を踏む可能性は高い。ただし上のとおり実測はできていない
- **テストは無い。** ここはフック層ではないが、`ime_indicator` は前面ウィンドウと他プロセスの IME を相手にする層で、単体テストの対象外である（[AGENTS.md](../../../AGENTS.md) ワークフロー 6 は `keymap.rs` / `config.rs` を対象としている）。確認は受け入れの M-1 で行う
