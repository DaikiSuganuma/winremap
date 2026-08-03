# ADR 0071: `--debug` の出力経路で常駐を殺さない（QuickEdit と、書き込み失敗の panic）

- 作成日: 2026-08-03
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- ステータス: 採用（2026-08-03 の受け入れ（[v0.8 §7](../03_acceptance-checklist.md)）で見つかった 2 件への手当て。[ADR 0068](0068-debug-console.md) が導入した自前コンソールの、詰め切れていなかった部分である）
- 関連: [ADR 0068](0068-debug-console.md)（`--debug` の専用コンソール）、[ADR 0062](../../v0.6/decisions/0062-detach-console-when-resident.md)（端末を手放しても効き続ける）、[ADR 0029](../../v0.2/decisions/0029-attach-console-and-tray-log-window.md)（コンソールへの出力とログウィンドウ）、[ADR 0016](../../v0.1/decisions/0016-debug-log-post-thread-message.md)（フックからログへの経路）、[ADR 0070](0070-agent-led-acceptance.md)（この 2 件を見つけた受け入れの形）
- 公式: [`SetConsoleMode`](https://learn.microsoft.com/en-us/windows/console/setconsolemode)（`ENABLE_QUICK_EDIT_MODE` は `ENABLE_EXTENDED_FLAGS` と併せて設定する）／[Low-level keyboard hook](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)（`LowLevelHooksTimeout` を超えた呼び出しは捨てられる）／[`std::io::Stdout`](https://doc.rust-lang.org/std/io/struct.Stdout.html)（`println!` は書き込み失敗で panic する）

## 背景

[ADR 0068](0068-debug-console.md) で `--debug` は自前のコンソールを持った。**その窓が、2 通りのやり方で常駐そのものを殺していた。** どちらも 2026-08-03 の受け入れで人が踏んで見つかったもので、自動テストは 1 つも捕まえていない。

### 1. 窓を 1 回クリックするとリマップが止まる

`AllocConsole` で作ったコンソールは **QuickEdit が既定で有効**である。窓の中を 1 回クリックすると選択モードに入り、**そのコンソールへの書き込みが全部ブロックされる**。

止まるのはログだけではない。書き込みでブロックされたスレッドはメッセージを汲まなくなり、低レベルフックの呼び出しが `LowLevelHooksTimeout`（既定 300 ms）を超え、**Windows がフックの呼び出しを捨て始める**。受け入れの記録に残った証拠がこれである — 22 秒間、キーの行が 1 つも出ていない:

```
14:50:22.742 [入力]   a ↑
14:50:30.576 [前面] application 指定値: "winremap.exe" …   ← 窓をクリックした
…
14:50:44.326 [入力]   LCtrl ↓                                ← Enter で選択を解いた後
```

**不変条件 1（フックを止めない）に対する違反が、利用者の 1 クリックで起きる。** しかも症状は「WinRemap が不安定」にしか見えず、この回は原因に辿り着くまでに M-1 を 2 度やり直している。

### 2. リダイレクトされた出力の書き込み失敗で panic して死ぬ

```powershell
.\winremap.exe --debug > log.txt
```

PowerShell の `>` はネイティブ exe の出力を**いったんパイプで受けて**自分でファイルへ書く。WinRemap は `windows` サブシステムなので PowerShell は終了を待たず、**コマンドが「終わった」時点でそのパイプを閉じる**。次にログを書いた WinRemap は:

```
thread 'main' panicked at library\std\src\io\stdio.rs:1166:9:
failed printing to stdout: パイプを閉じています。 (os error 232)
```

`println!` は**書き込みに失敗すると panic する**。ログを配れなくなっただけで、トレイアプリごと落ちた。[ADR 0062](../../v0.6/decisions/0062-detach-console-when-resident.md) の「端末を手放しても効き続ける」に正面から反する。

## 決定

### 1. 自前のコンソールでは QuickEdit を切る

`open_debug_console()` が `AllocConsole` の直後に `GetConsoleMode` → `ENABLE_QUICK_EDIT_MODE` を落として `ENABLE_EXTENDED_FLAGS` を立てる → `SetConsoleMode`。**読んだモードの他のビットは持ち越す**（終了時の待ちが使う行入力を消さないため）。

失うのは**ドラッグでの選択**である。窓のメニュー（Alt+Space → 編集 → 範囲指定）からは今までどおり選択・コピーできる。**選択の手軽さはフックより軽い** — 迷ったときの優先順位（安定性 ＞ 単純さ ＞ 機能）そのままの判断である。

**触るのは自分で開いたコンソールだけ**とする。`AttachConsole` で借りた親のコンソールのモードは**変えない** — あれは利用者のシェルの持ち物で、WinRemap が終わったあとも QuickEdit が切れたままになるのは越権である。

### 2. コンソールへの書き込みは失敗しても panic しない

`println!` / `eprintln!` の直接呼び出しをやめ、`writeln!` の結果を捨てる `print_line` / `print_error_line` を通す。

**配れなかった記録は失われた記録であって、リマップを止める理由ではない。** これは「ログを黙って捨てる」ことを積極的に選ぶ決定である — 相手が受け取れないと分かっている経路（閉じたパイプ）に対して、報告する先も無い。

### 3. **PowerShell の `>` は直せないので、手順のほうを直す**

決定 2 で panic は消えるが、**`>` で受けたファイルは空のまま**である。PowerShell は GUI サブシステムのプロセスを待たないので、起動直後にパイプを畳んでしまう。WinRemap 側にできることは無い。

したがって受け入れ項目 C-4 の手順を、**実際に転記できる形**に書き換える:

| 経路 | 結果 |
|---|---|
| `winremap.exe --debug > log.txt`（PowerShell） | 窓は開かない・常駐は続く。**ファイルは空**（PowerShell がパイプを閉じるため） |
| `cmd /c "winremap.exe --debug > log.txt"` | 通る |
| `Start-Process winremap.exe -ArgumentList '--debug' -RedirectStandardOutput log.txt` | 通る。**UI テストがこれを使っている** |

## 却下した代替案

- **QuickEdit ではなく、書き込みを別スレッドへ逃がす**（ログをキューに積み、専用スレッドが書く）: ブロックしても他が止まらない、という点では筋がよい。だが**キューが詰まったときに何を捨てるかという判断が増え**、`--debug` のためだけに常駐スレッドが 1 本増える。QuickEdit を切れば**ブロックする状況自体が消える**ので、そちらが小さい
- **書き込みにタイムアウトを付ける**: `WriteFile` は同期で、コンソールの選択モードは非同期 I/O でも解けない。実現手段が無い
- **親のコンソールでも QuickEdit を切る**: 症状は同じだが、他人の設定を勝手に変えることになる。`--debug` が親のコンソールに書くのは v0.7 以前の話で、今は自前の窓を使う（ADR 0068）
- **書き込み失敗を検知したら以後の出力をやめる**（`HAS_CONSOLE` を落とす）: 無駄な書き込みは減るが、**一時的な失敗と恒久的な失敗を区別できない**。失敗のたびに諦めるのは、失敗のたびに落ちるのと同じ種類の乱暴さである
- **`--debug` のときだけ panic hook で握り潰す**: 落ちなくはなるが、**落ちる理由を残したまま**である。他の panic まで飲み込む
- **PowerShell の `>` でもファイルに残るようにする**（起動直後に flush する、など）: 受け入れの実測では**1 行も届いていない**。パイプはプロセスが常駐に入る前に閉じられており、WinRemap が早く書いても間に合わない

## 影響・補足

- **`--debug` の窓でドラッグ選択ができなくなる。** 窓のメニューからは選択できる。ログをファイルに残したいなら上表の 2 経路か、トレイの「ログを表示」
- **触るのは `--debug` の窓だけ。** 無印の起動は窓を開かないので、この変更は何も踏まない
- **UI テストへの影響は無い。** テストは `Start-Process -RedirectStandardOutput` で、コンソールを開かない経路を通る（`stdout_is_captured` が先に効く）
- **自動テストが捕まえていなかった 2 件である。** クリックで凍る件は「人が窓をクリックする」ことが前提で、`tests/ui` の想定に無い。受け入れが人の手で行われる理由がここに出ている（[ADR 0070](0070-agent-led-acceptance.md)）
