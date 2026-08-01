# 前面アプリ検出の取りこぼし — 原因調査（2026-08-01）

- 作成日: 2026-08-01
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／実測はゲスト VM `winremap-test`（Windows 11 Pro、日本語環境）
- 対象: [v0.5 からの持ち越し](../../v0.6/01_development-plan.md)（ログウィンドウを開いている間、切り替えた先のアプリが前面アプリ行に出ない）
- 実測に使った道具: [`tests/ui/guest/06-foreground-line.ps1`](../../../tests/ui/guest/06-foreground-line.ps1)（症状の判定）、[`tests/ui/guest/probe-foreground-watch.ps1`](../../../tests/ui/guest/probe-foreground-watch.ps1) と [`foreground-listener.ps1`](../../../tests/ui/guest/foreground-listener.ps1)（原因の切り分け）
- 公式: [`SetWinEventHook`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)／[`WinEventProc`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-wineventproc)（`hwnd` 引数の意味）／[`GetForegroundWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow)／[Event Constants](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants)（`EVENT_SYSTEM_FOREGROUND`）

## 結論

**イベントは 1 件も取りこぼしていない。間違っていたのは、イベントを受けたあとに調べ直した答えのほうである。**

[`src/window.rs`](../../../src/window.rs) の `on_foreground_changed` は、**イベントが運んでくる `hwnd` を意図的に捨てて** `GetForegroundWindow()` を引き直す。

```rust
// Query the current foreground window instead of trusting the event's
// HWND: events can arrive out of order and the latest state is what the
// next key event will actually be delivered to.
refresh_foreground_cache();
```

この引き直しが**切り替えの確定より早く走ることがある**。そのとき返るのは**直前まで前面だったウィンドウ**で、キャッシュはその名前で上書きされる。**訂正の機会は無い** — 次の切り替えが起きるまで、フックは間違ったアプリ名でキーマップを選び続ける。

## 測り方

同じ `EVENT_SYSTEM_FOREGROUND` を購読する**独立したクライアントを別プロセスで**動かし（`foreground-listener.ps1`）、WinRemap の `--debug` 記録と 1 対 1 で突き合わせた。リスナーは**イベントの `hwnd` をそのまま使う**（引き直さない）ので、両者の差は**その 1 点だけ**である。

この道具立てが必要だった理由は、内側からは 2 つの事象が同じに見えるためである。

| 外から見た症状 | ありうる原因 | 直し方 |
|---|---|---|
| 前面アプリ行に出ない | **システムがイベントを配っていない** | 別の信号（ポーリング等）に替える |
| 同上 | **配られているが、こちらの答えが間違っている** | 答えの出し方を直す |

## 実測（2026-08-01 10:23〜10:25、1 回の実行）

| | |
|---|---|
| リスナーが受け取ったイベント | **18 件** |
| WinRemap が報告した件数 | **18 件**（取りこぼし **0**） |
| **名前が食い違った件数** | **4 件（22%）** |

食い違った 4 件は次のとおりで、**すべて「直前まで前面だったウィンドウ」を答えている**。

| WinRemap の時刻 | WinRemap が報告 | 実際に前面になったもの | 差 |
|---|---|---|---|
| 10:23:54.630 | `powershell.exe` | `explorer.exe`（54.648） | **−18 ms** |
| 10:24:05.842 | `explorer.exe` | `Notepad.exe`（05.858） | **−16 ms** |
| 10:24:24.669 | `winremap.exe` | `explorer.exe`（24.686） | **−17 ms** |
| 10:25:00.553 | `winremap.exe` | `explorer.exe`（00.564） | **−11 ms** |

**WinRemap のコールバックは、リスナーより一貫して 11〜18 ms 早く走っている。** 早く走るほど、切り替えが確定する前に引き直すことになる。WinRemap のフックのほうが先に登録されている（プロセス起動が先）ことと整合する。

### 利用者に見える結果

10:24:05.842 の誤りが、そのまま次のような形で出た。

```
10:24:05.842 [window]  application = "explorer.exe" — matching keymaps: (none)   ← 実際は Notepad が前面
10:24:13.204 [decided] C-h (BS 0x08) → passed through                            ← notepad のルールが選ばれない
```

メモ帳の文字は `abc` のまま（ルールが効いていれば `abcx`）。**この 1 件は WinRemap のウィンドウを 1 枚も開いていない段（対照）で起きている。**

## 前の見立ての訂正

**「ログウィンドウを開いていると起きる」は誤りだった。**

| いつ | 見立て | 根拠にしたもの |
|---|---|---|
| v0.5・v0.7 §3.5.1 | ウィンドウを開いている間だけ起きる | ウィンドウを開いた状態で 3 回連続で観測し、開いていない対照では起きなかった |
| **本調査** | **ウィンドウの有無と関係ない競合状態** | 4 段階のうち**対照（ウィンドウ無し）でも起きた**。18 件中 4 件という発生率 |

**発生率 22% の事象を、条件を変えながら数回ずつ観測すれば、条件と相関しているように見える。** ウィンドウを開くと切り替えの回数が増える（トレイ操作・ウィンドウの前面化）ぶん当たりやすくはなるが、それは条件ではなく試行回数である。

v0.6 が開発機で再現しなかったこと（[v0.6 §1](../../v0.6/01_development-plan.md)）も、これで説明が付く。**再現しないのではなく、その日は当たらなかった。**

## 直し方（提案）

**イベントが運んでくる `hwnd` を使う。** `WinEventProc` の `hwnd` は「前面になったウィンドウ」そのもので、引き直す必要が無い。

```rust
unsafe extern "system" fn on_foreground_changed(
    _hook: HWINEVENTHOOK, _event: u32, hwnd: HWND,
    id_object: i32, id_child: i32, _thread: u32, _time: u32,
) {
    // EVENT_SYSTEM_FOREGROUND はウィンドウ自身についてのみ意味を持つ
    if id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 { return; }
    refresh_foreground_cache_for(hwnd);
    ...
}
```

- 現行の `refresh_foreground_cache()`（`GetForegroundWindow()` 版）は**起動時のシード用に残す**（[`src/main.rs`](../../../src/main.rs) の呼び出し）
- 元のコメントが心配していた「イベントが順不同で届く」については、公式が **`WINEVENT_OUTOFCONTEXT` のイベントは順序が保証される**と書いている（[SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook) の `WINEVENT_OUTOFCONTEXT` の項）。順序が保たれるなら、**最後に届いたイベントの `hwnd` が最新の前面ウィンドウ**である
- `hwnd` が無効な場合（ウィンドウ破棄との競合）は現行どおり「取得できませんでした」の経路へ落ちる

### 副次的に見つかった問題（別件）

[`src/gui/config_window.rs`](../../../src/gui/config_window.rs) の「今の前面アプリから取得」（B4）が **GUI スレッドから `refresh_foreground_cache()` を呼んでいる**。キャッシュは `thread_local!` なので、**書き込まれるのは GUI スレッド側の複製**で、フックが読むメインスレッドのキャッシュは更新されない。B4 は直後に同じスレッドで読み返すため**ボタンの機能としては正しく動く**が、「ここで更新したからフックも新しい値を見る」と読める形になっている。上の修正を入れるときに、**関数名か注釈でどちらのキャッシュを触るのかを明示する**のが安全である。

## 検査への申し送り

**`06-foreground-line` の表明は、発生率 22% の事象に対して 1 回の切り替えで判定している。** つまり**緑になることがある**。検査として成立させるなら、**切り替えを N 回繰り返して食い違いを数える**形（本ノートの測り方をそのまま検査にする）へ寄せるべきである。修正を入れるなら、**修正前に「N 回中 M 回食い違う」を測り、修正後に 0 になることを見る**のが順序として正しい。
