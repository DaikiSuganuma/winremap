# ADR 0068: `--debug` は自前のコンソールを開き、閉じられるまで残す（ADR 0029 の決定 1 を覆す）

- 作成日: 2026-08-02
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）
- ステータス: 採用（**オーナー決定 2026-08-02**「`AllocConsole` 化は閉じるまで残す処理にしたい」）
- 関連: **[ADR 0029](../../v0.2/decisions/0029-attach-console-and-tray-log-window.md)（本 ADR が一部を覆す）**、[ADR 0062](../../v0.6/decisions/0062-detach-console-when-resident.md)（常駐時にコンソールを手放す）、[ADR 0031](../../v0.2/decisions/0031-notify-module-unsafe-allowlist.md)（`notify.rs` を `unsafe` の隔離先にした）、[ADR 0058](../../v0.5/decisions/0058-log-readability.md)（ログの行の形）、[v0.7 受け入れチェックリスト §6.3](../../v0.7/03_acceptance-checklist.md)（重なる件の経緯）
- 公式: [`AllocConsole`](https://learn.microsoft.com/en-us/windows/console/allocconsole)／[`AttachConsole`](https://learn.microsoft.com/en-us/windows/console/attachconsole)／[`FreeConsole`](https://learn.microsoft.com/en-us/windows/console/freeconsole)／[`HandlerRoutine`](https://learn.microsoft.com/en-us/windows/console/handlerroutine)（`CTRL_CLOSE_EVENT` は拒否できない）／[`ReadConsoleW`](https://learn.microsoft.com/en-us/windows/console/readconsole)／[windows_subsystem 属性](https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute)

## 背景

[ADR 0029](../../v0.2/decisions/0029-attach-console-and-tray-log-window.md) は `AllocConsole`（案 B）を「トレイのログウィンドウが同じ役割をより良い UX で果たすため不要」として却下し、`AttachConsole(ATTACH_PARENT_PROCESS)` を採った。**当時の判断は正しい。** 変わったのは、その後に**この 2 つでは満たせない用途**が出てきたことである。

> **起動時の挙動と終了時の挙動をログで確認したい。**（オーナー 2026-08-02）

現状はどちらもできない。

- **ログウィンドウは起動後にしか開けない。** 起動時のログは、開いたときにはもう終わっている
- **`AttachConsole` はシェルと画面を共有する。** WinRemap は `windows` サブシステムなのでシェルは終了を待たずにプロンプトを描き直し、**ログの行とプロンプトが同じ行に重なる**（[v0.7 §6.3](../../v0.7/03_acceptance-checklist.md)）。カーソル位置を握っているのは相手側なので、**WinRemap 側だけでは直せない**
- **終了時のログはさらに見えない。** `AttachConsole` の場合、終了時の出力はプロンプトの下に紛れる

## 決定

1. **`--debug` のときは `AllocConsole` で自前のコンソールを開く。** 画面バッファを他の誰とも共有しないので、**プロンプトと重ならない。起動の最初の 1 行から出せる**
2. **終了時は、コンソールが閉じられるまでプロセスを残す**（オーナー決定）。終了処理のログをすべて書き終えたあと、最後に 1 行

   > `--- WinRemap は終了しました。このウィンドウを閉じるか、Enter を押してください ---`

   を出し、**`CONIN$` からの読み取りで待つ**。窓が閉じられれば `CTRL_CLOSE_EVENT` で落ち、Enter が押されればそのまま終了する。**これをしないと、`AllocConsole` の窓はプロセス終了と同時に消え、終了時のログは読めない**（この決定が無ければ、この ADR は目的を果たさない）
3. **待つのは利用者が起こした終了だけにする。** サインアウト・シャットダウン（`WM_ENDSESSION`）では待たない。**OS の終了処理を待たせるのは有害**であり、そこで読ませる相手も居ない
4. **リダイレクトされているときは `AllocConsole` しない。** `winremap --debug > out.txt` や、パイプで受けている場合は**従来どおりリダイレクト先へ書く**。UI テストは `--debug` の出力をファイルへ流して読んでおり、**ここを変えると自動テストが全部落ちる**。決定 2 の待ちも行わない（誰も Enter を押せない）
5. **`--debug` 以外は何も変えない。** `--help` / `--version` / 起動時のエラーは、これまでどおり `AttachConsole` で**起動元のターミナルに**出す（ADR 0029 の意図）。常駐に入る前に手放すのも従来どおり（[ADR 0062](../../v0.6/decisions/0062-detach-console-when-resident.md)）
6. **トレイのログウィンドウは残す。** これは一般利用者の閲覧手段であり、置き換えるものではない（ADR 0029 の決定 2）

## 理由

**ADR 0029 が案 B を却下した根拠は「ログウィンドウで足りる」だった。** その前提が崩れたのは、ログウィンドウが**プロセスの寿命の内側でしか存在できない**ためである。起動時のログは開く前に流れ、終了時のログはウィンドウごと消える。**自前のコンソールは、この 2 か所だけを埋める。**

決定 2（閉じるまで残す）は、この ADR の中身そのものである。`AllocConsole` で開いた窓は**その プロセスが最後の 1 つ**なので、プロセスが終わった瞬間に窓ごと消える。終了時のログを出す実装をどれだけ丁寧に書いても、**読める時間が 0 である。**

## 却下した代替案

- **`AttachConsole` のまま、出力の書き方を工夫する**: できない。プロンプトを描き直しているのは PSReadLine であり、カーソル位置を握っているのは相手側である
- **終了時にログをファイルへ書く**: [不変条件 6](../../../AGENTS.md)（キーロガー化の禁止）に照らして、`--debug` のログは**キー名を含む**ためディスクに残さない、という ADR 0029 の判断を維持する
- **終了時に一定時間だけ待つ（5 秒など）**: 読み終わる保証が無く、読み終わっても待たされる。**閉じるまで**のほうが単純で、利用者が制御できる
- **常に `AllocConsole` する（`--debug` 以外でも）**: 常駐ツールが起動のたびに黒い窓を出す。ADR 0029 が最初に消したものであり、戻さない

## 影響・補足

- **受け入れの [H-10](../../v0.7/03_acceptance-checklist.md) が変わる。** 「`--debug` は起動元のターミナルを閉じると終了する」は成立しなくなる（自前の窓を持つため）。**代わりに「`--debug` の窓を閉じると終了する」**を確認する。無印の挙動（ターミナルを閉じても常駐が続く）は変わらない
- **`--debug` の窓を閉じると WinRemap が落ちる。** `CTRL_CLOSE_EVENT` はハンドラが `TRUE` を返しても終了を止められない（ADR 0062 で確認済み）。デバッグ用途では自然な挙動として受け入れ、文書に書く
- **`unsafe` の追加は `notify.rs` の中で収まる**（[ADR 0031](../../v0.2/decisions/0031-notify-module-unsafe-allowlist.md) の許可範囲）。不変条件 3 の改訂は不要
- **ヘルプサイトの FAQ を書き換える。** v0.7 で足した回避策（`Start-Process ... -NoNewWindow -Wait`）は、この版で**不要になる**（[v0.7 §6.3](../../v0.7/03_acceptance-checklist.md)）
- **文字コードに注意する。** 新しいコンソールの出力コードページは既定で OEM である。日本語のログを出しているので、`SetConsoleOutputCP(CP_UTF8)` 相当を明示するか、`WriteConsoleW` で書く。**ここを外すと全部文字化けする**
