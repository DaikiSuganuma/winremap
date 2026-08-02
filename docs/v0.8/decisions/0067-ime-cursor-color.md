# ADR 0067: IME の状態をマウスカーソルの色で示す（`SetSystemCursor` と、異常終了の検出）

- 作成日: 2026-08-02
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）
- ステータス: 採用（**オーナー決定 2026-08-02**「マウスカーソルの色は `SetSystemCursor` で」）。**異常終了時の扱いは §「決定」3〜5 が提案で、承認が要る**
- 関連: [ADR 0033](../../v0.2/decisions/0033-ime-status-across-input-threads.md)（IME 状態をスレッドをまたいで取る）、[ADR 0016](../../v0.1/decisions/0016-debug-key-logging.md)（フックからは Win32 を呼ばない）、[ADR 0031](../../v0.2/decisions/0031-notify-module-unsafe-allowlist.md)（`unsafe` の隔離先を増やすときの前例）、[ADR 0061](../../v0.6/decisions/0061-packaged-config-path.md)（パッケージ版の書き込み先）、[ADR 0065](../../v0.7/decisions/0065-foreground-window-from-the-event.md)（前面ウィンドウの通知経路）
- 公式: [`SetSystemCursor`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setsystemcursor)（**渡したカーソルの所有権を取り、破棄する**）／[`SystemParametersInfoW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)（`SPI_SETCURSORS` はレジストリからカーソル一式を読み直す）／[`CopyIcon`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-copyicon)／[`SetUnhandledExceptionFilter`](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-setunhandledexceptionfilter)／[`HandlerRoutine`](https://learn.microsoft.com/en-us/windows/console/handlerroutine)（`CTRL_CLOSE_EVENT` は拒否できない）

## 背景

IME のオン・オフは、既存のインジケーター（キャレット付近の小さな表示）で示している。オーナーの要望は**マウスカーソルの色**でも分かるようにすることである。

`SetSystemCursor` は**セッション全体のカーソルを差し替える** API である。これは長所と短所が表裏になっている。

- **効果が確実**：本物のカーソルが変わる。オーバーレイのような追従の遅れが無く、**管理者権限のウィンドウの上でも効く**（UIPI に阻まれない数少ない部分）
- **プロセスの外に残る状態**：差し替えはシステム側の状態であり、**WinRemap が死んでも元に戻らない**

したがって設計上の主題は「色をどう変えるか」ではなく、**残った状態をどう畳むか**である。

## 決定

1. **`SetSystemCursor` で差し替える。** 対象は `OCR_NORMAL`（矢印）と **`OCR_IBEAM`（テキストの I ビーム）の 2 つ**を最低限とする。**IME を気にする場面はまさに文字を打っている最中**であり、そこで出ているのは I ビームだからである。矢印だけを変えると「入力欄の上では色が変わらない」という中途半端な機能になる
2. **復元は `SystemParametersInfoW(SPI_SETCURSORS, 0, NULL, SPIF_SENDCHANGE)` で行う。** 元のハンドルを取っておいて戻すことはできない — `SetSystemCursor` は**渡したハンドルを破棄する**ので、保存した「元のカーソル」は差し替えの時点で無効になる。レジストリから読み直すのが唯一の正しい戻し方であり、**利用者が独自のカーソルテーマを使っていてもそのテーマに戻る**

### 異常終了への備え（**この ADR の本題**）

3. **プロセス内で走れる終了経路は、すべて復元を通す。**

   | 経路 | 手当て |
   |---|---|
   | トレイの「終了」・通常終了 | 終了処理で復元 |
   | Rust の panic | `std::panic::set_hook` で復元してから既定のハンドラへ落とす |
   | 未処理の構造化例外（アクセス違反など） | `SetUnhandledExceptionFilter` で復元してから既定へ渡す |
   | サインアウト・シャットダウン | `WM_ENDSESSION` で復元 |
   | `--debug` のコンソールが閉じられた | 既存の `CTRL_CLOSE_EVENT` 経路で復元（[ADR 0062](../../v0.6/decisions/0062-detach-console-when-resident.md)） |

4. **強制終了（`TerminateProcess`、タスクマネージャー、電源断）は、プロセス内では何も走らない。** ここは**痕跡ファイル**で畳む。

   - カーソルを差し替えた瞬間に、設定ファイルと同じ解決済みディレクトリ（[ADR 0061](../../v0.6/decisions/0061-packaged-config-path.md)）へ**空のファイルを 1 つ作る**（`cursor-restore-pending`）。復元と同時に消す
   - **次回の起動時にこのファイルが残っていたら、前回は異常終了している。** 起動時に無条件で `SPI_SETCURSORS` を打って復元し、**「前回、WinRemap は異常終了しました」とログとダイアログで知らせる**
   - 中身は書かない。**キー入力に関する情報を一切含まない**（[AGENTS.md 不変条件 6](../../../AGENTS.md)）ので、キーロガー化の禁止には触れない

5. **「今まさに異常終了している」ことは、状態の組み合わせで分かるようにし、それを文書化する。**

   > **カーソルの色が変わったままで、トレイに WinRemap が居ない。** これが「異常終了した」という意味である。

   これはヘルプサイトと README に書く。プロセスが消えたあとに何かを表示する手段はプロセス内には無いので、**利用者が自力で判断できる形にすることが答えになる**。色は「オフ＝既定のまま／オン＝色付き」とし、**既定の状態を差し替えない**のもこのためである（残るとしたら必ず「色付き」であり、正常時の見た目と混同されない）。

## 却下した代替案

- **カーソルに追従するオーバーレイ**: プロセスが死ねば必ず消えるので安全だが、追従が遅れて見え、**管理者権限のウィンドウの上に出せない**。ADR 0065 で直したばかりの「前面が絡む処理」をもう 1 つ増やすことにもなる
- **監視プロセスを常駐させて、WinRemap が消えたら復元する**: 確実だが**プロセスが 2 つになる**。単一インスタンスの仕組み（`hook::acquire_single_instance`）と噛み合わず、「常駐ツールが増えた」という利用者の印象も悪い。得られるのは「強制終了直後に自動で戻る」だけで、決定 4 の次回起動時復元との差は**その 1 回のセッションの間だけ**である
- **カーソルテーマ一式（十数個）を差し替える**: 見た目の一貫性は上がるが、用意する画像と復元の失敗面が増える。**まず 2 つで足りるかを実際に使って確かめる**（足りなければ足す）
- **痕跡ファイルを置かない**: 「残ったカーソル」と「IME がオン」を利用者が区別できない。**異常終了したことが分かる**という要件を満たさない

## 影響・補足

- **`unsafe` の隔離先が 1 つ増える。** 新しいモジュール（`src/cursor.rs` を想定）に閉じ込め、[AGENTS.md 不変条件 3](../../../AGENTS.md) の許可リストへ追加する。前例は [ADR 0031](../../v0.2/decisions/0031-notify-module-unsafe-allowlist.md)（`notify.rs`）で、**同じ理由・同じ形**である
- **フックからは呼ばない。** 差し替えは IME 状態の変化を受け取っている既存の経路（メッセージループ側）で行う。[不変条件 2](../../../AGENTS.md) は変わらない
- **`SetSystemCursor` に渡すハンドルは複製にする。** 自前のカーソルリソースをそのまま渡すと破棄され、2 回目の差し替えで無効なハンドルを渡すことになる。`CopyIcon` した複製を毎回渡す
- **受け入れに項目が増える。** 「異常終了させてカーソルが残ること」「次に起動すると戻り、異常終了が知らされること」を実際に殺して確かめる（`Stop-Process -Force`）。これは v0.8 の対話式受け入れハーネスの最初の顧客になる
- **既定はオフにする。** システム全体のカーソルを触る機能なので、設定で明示的に有効にした人にだけ働く
