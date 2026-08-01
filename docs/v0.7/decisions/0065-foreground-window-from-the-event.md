# ADR 0065: 前面ウィンドウはイベントが運ぶ HWND で決める（引き直さない）

- 作成日: 2026-08-01
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／判断の根拠は §「実測」のとおり修正前後の実測
- ステータス: 採用（オーナー決定 2026-08-01「v0.7 で直す」）
- 関連: [ADR 0016](../../v0.1/decisions/0016-debug-key-logging.md)（フックは I/O をしない。前面アプリの取得をフックから外した理由）、[ADR 0033](../../v0.2/decisions/0033-ime-status-across-input-threads.md)（IME 状態も前面ウィンドウを起点にする）、[ADR 0058](../../v0.5/decisions/0058-log-readability.md)（前面アプリ行の形）、[ADR 0064](0064-winapp-cli-for-ui-tests.md)（この不具合を捕まえた検査の基盤）
- 実測: [前面アプリ検出の取りこぼし — 原因調査](../notes/20260801_foreground-race.md)（独立クライアントとの突き合わせ）、[`tests/ui/guest/probe-foreground-watch.ps1`](../../../tests/ui/guest/probe-foreground-watch.ps1)（修正前後の計測）、[v0.7 開発計画 §3.5.1](../01_development-plan.md)
- 公式: [`SetWinEventHook`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)（`WINEVENT_OUTOFCONTEXT` はイベントの順序を保証する）／[`WinEventProc`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-wineventproc)（`hwnd`・`idObject`・`idChild` の意味）／[`GetForegroundWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow)／[Event Constants](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants)（`EVENT_SYSTEM_FOREGROUND`）

## 背景

WinRemap は前面アプリの exe 名をキャッシュし、キーボードフックはそれを見てアプリ別キーマップを選ぶ。フックから Win32 を呼べない（[AGENTS.md 不変条件 2](../../../AGENTS.md)）ため、更新は `EVENT_SYSTEM_FOREGROUND` のコールバックで行う。

そのコールバックは、**イベントが運んでくる `hwnd` を捨てて `GetForegroundWindow()` を引き直していた。**

```rust
// Query the current foreground window instead of trusting the event's
// HWND: events can arrive out of order and the latest state is what the
// next key event will actually be delivered to.
refresh_foreground_cache();
```

理屈は通っている。**ただし前提が 1 つ間違っていた** — 引き直した答えが「今」である保証が無い。

### 症状

v0.5 から持ち越されていた観測（「ログウィンドウを開いている間、切り替えた先のアプリが前面アプリ行に出ない」）の正体がこれである。行が出ないだけでなく、**同じキャッシュをフックが読んでいるので、アプリ別キーマップが選ばれない。**

```
10:24:05.842 [window]  application = "explorer.exe" — matching keymaps: (none)   ← 実際は Notepad が前面
10:24:13.204 [decided] C-h (BS 0x08) → passed through                            ← notepad のルールが選ばれない
```

## 実測

同じイベントを購読する**独立したクライアントを別プロセスで**動かし（[`foreground-listener.ps1`](../../../tests/ui/guest/foreground-listener.ps1)）、WinRemap の記録と 1 対 1 で突き合わせた。リスナーは**イベントの `hwnd` をそのまま使う**ので、両者の違いはその 1 点だけである。

| | 修正前 |
|---|---|
| リスナーが受け取ったイベント | 18 件 |
| WinRemap が報告した件数 | 18 件（**取りこぼし 0**） |
| **名前が食い違った件数** | **4 件（22%）** |

食い違った 4 件は**すべて「直前まで前面だったウィンドウ」**であり、WinRemap のコールバックは**リスナーより 11〜18 ms 早く**走っていた。フックの登録が先なぶん、最も早く走り、最も外しやすい位置にいる。

**取りこぼしゼロという数字が決め手である。** イベントは全部届いていた。壊れていたのは受け取り方ではなく、受け取ったあとの調べ直しだった。

### 修正前後（同じ計測、同じスクリプト、24 回の切り替え）

`90-probe-foreground` を「Explorer ↔ Notepad を 12 回ずつ、ウィンドウ無しとログウィンドウ有りの 2 条件」で回し、**1 回の切り替えごとに**リスナーと WinRemap の答えを突き合わせた。

| | 修正前 | 修正後 |
|---|---|---|
| ウィンドウ無し（12 回） | **1 件が食い違い** | **0 件** |
| ログウィンドウ有り（12 回） | **1 件が食い違い** | **0 件** |
| アプリ別キーマップ（打鍵で確認） | — | **効いた**（ウィンドウ有り・閉じたあと とも） |

修正前の 2 件はどちらも**切り替え元**の名前だった（`A/2` で Notepad へ切り替えて `explorer.exe`、`B/0` で Notepad へ切り替えて `winremap.exe`）。**片方はウィンドウを 1 枚も開いていない条件で起きている** — 発生率は測るたびに違う（22% と 8%）が、条件ではないことは 2 回の計測で一致している。

**スクリプトは修正前後で 1 文字も変えていない。** 変えたのはバイナリだけである。

## 決定

### 1. `hwnd` を使う

`WinEventProc` の `hwnd` は「前面になったウィンドウ」そのものである。競合する相手が無い。

```rust
if id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 {
    return;
}
if hwnd.is_invalid() {
    refresh_foreground_cache();
} else {
    set_foreground_cache(hwnd);
}
```

元のコメントが心配していた「イベントが順不同で届く」については、公式が **`WINEVENT_OUTOFCONTEXT` のイベントは順序が保証される**と書いている。順序が保たれるなら、**最後に届いたイベントの `hwnd` が今の前面ウィンドウ**である。

### 2. `idObject` / `idChild` で絞る

`EVENT_SYSTEM_FOREGROUND` はウィンドウ自身（`OBJID_WINDOW` / `CHILDID_SELF`）について発生する。それ以外が届いたらウィンドウの切り替えではないので無視する。**従来は引数を 1 つも見ていなかった**ため、この区別が存在しなかった。

### 3. `GetForegroundWindow()` 版は起動時のシード専用として残す

起動時には学ぶべきイベントがまだ無いので、聞くしかない。そのときは**切り替えの最中ではない**ので、引き直しの危険も無い。

### 4. 設定ウィンドウの「今の前面アプリから取得」は専用の関数にする

調査中に見つかった別件である。B4 は **GUI スレッドから `refresh_foreground_cache()` を呼んでいた**が、キャッシュは `thread_local!` なので**書かれるのは GUI スレッド側の複製**で、フックが読むメインスレッドのキャッシュは変わらない。ボタンの機能としては直後に同じスレッドで読み返すため正しく動いていたが、**「ここで更新したからフックも新しい値を見る」と読める形**だった。

キャッシュに触れない `query_foreground_exe()` を足し、B4 はそれを使う。**利用者は 3 秒かけて対象ウィンドウを指したところなので、ここでは引き直しで正しい。**

## 受け入れた制約

| # | 制約 | 扱い |
|---|---|---|
| 1 | **単体テストで守れない。** `SetWinEventHook` のコールバックは実機の切り替えでしか動かない | 回帰は UI テスト側に置く（`06-foreground-line` と `90-probe-foreground`）。**修正前の食い違い率を測ってから直した**のはこのためで、数字が無ければ「直った」と言えない |
| 4 | **§4 の B4 を押す検査が 1 つも無い。** 設定ウィンドウの「今の前面アプリから取得」は UI テストが触っていない | コンパイルと読みでしか担保できていないことを明示する。**受け入れ（H-3 で設定ウィンドウを開くとき）に押してみること** |
| 2 | **イベントの順序保証に乗る。** 順序が崩れる環境があれば、最後のイベントが最新とは限らない | 公式の保証（`WINEVENT_OUTOFCONTEXT`）に乗る。崩れた場合の症状は**修正前と同じ**（一時的に前のアプリを指す）で、悪化はしない |
| 3 | **`hwnd` が null のときは結局聞くことになる** | デスクトップ自身にフォーカスがある場合など。そのときは切り替えの最中ではないので、引き直しで問題ない |

## 却下した案

| 案 | 却下理由 |
|---|---|
| **引き直しは残し、一定時間後にもう一度引く**（遅延して確認する） | 遅らせる時間は当て推量になり、そのあいだキーは間違ったキーマップで処理される。**イベントが正しい答えを持っているのに推測する理由が無い** |
| **フックの中で `GetForegroundWindow()` を呼ぶ** | 不変条件 2（フックから Win32 を呼ばない）に反する。ADR 0016 がこの構造を選んだ理由そのもの |
| **定期的にポーリングして差分を見る**（タイマー） | 常駐アプリの放置時 CPU を売りにしている（受け入れ H-4）。**イベントで足りるものにタイマーを足さない** |
| **`GetForegroundWindow()` の結果とイベントの `hwnd` が食い違ったら新しい方を採る** | 「新しい方」を判定する手段が無い。食い違いの正体は**古い方を読んでいること**なので、比較しても情報が増えない |

## 影響

- `src/window.rs`: `on_foreground_changed` が `hwnd` と `idObject`/`idChild` を使う。`set_foreground_cache(hwnd)` を新設し、`refresh_foreground_cache()` は起動時用に残す。`query_foreground_exe()` を追加
- `src/gui/config_window.rs`: B4 が `query_foreground_exe()` を使う（キャッシュに触れない）
- **利用者から見た変化**: アプリを切り替えた直後に、切り替え元のキーマップが使われることが無くなる。v0.5 から 3 バージョン持ち越された観測がここで閉じる
- ログの前面アプリ行が、切り替え先を正しく名指しするようになる（[ADR 0058](../../v0.5/decisions/0058-log-readability.md) の行が意図どおりに働く）

## 測り方から残す教訓

**「条件を変えて数回ずつ試す」は、確率的な不具合の前では嘘をつく。** この不具合は 3 バージョンにわたって「ログウィンドウを開いていると起きる」と記録されてきた。ウィンドウを開いた状態で 3 回観測し、開かない対照で 1 回観測すれば、22% の事象はそう見える。**回数を増やして数えるまで、条件と試行回数は区別できない。**

**独立した第 2 の実装が、内側からは見えない差を可視化した。** リスナーと WinRemap の違いは「イベントの `hwnd` を使うか、引き直すか」の 1 点だけで、だからこそ食い違いがそのまま原因を指した。[ADR 0064 §3](0064-winapp-cli-for-ui-tests.md) が `00-uia-actuation` を対照として残した理由と同じ構図である。
