# UI テスト自動化（VM ＋ winapp CLI）

> 元資料: windows-utility リポジトリの `test-vm/利用プロジェクト向けガイド.md`（利用側が守ること・書くこと）と
> `test-vm/セットアップ手順.md`（構築・設計判断）。
> 環境の構築・保守はそちらが正であり、本書は**このリポジトリ側の使い方**を記述する。
> **何をテストするかは本リポジトリ側に置く**（被テストアプリの取得・ビルド・検査を共通基盤に置かない）のが基盤側の方針である。

- 作成日: 2026-07-25 ／ 更新日: 2026-07-31（**全面改訂**。操作系を winapp CLI に統一し、AI エージェント版シナリオを廃止した — [ADR 0064](v0.7/decisions/0064-winapp-cli-for-ui-tests.md)）
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- 関連: [ADR 0064](v0.7/decisions/0064-winapp-cli-for-ui-tests.md)（操作系の決定）、[ADR 0053](v0.4/decisions/0053-test-inject-mode.md)（注入イベント受理モード）、[ADR 0055](v0.5/decisions/0055-accesskit-for-child-viewports.md)（子ビューポートを UIA に出す）、[winapp CLI 移行検討](v0.5/notes/20260727_winapp-cli-migration.md)（検証計画）
- 公式: [Windows App Development CLI](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/)／[winappCli ui-automation.md](https://github.com/microsoft/winappCli/blob/main/docs/ui-automation.md)

---

## 1. これは何か

VMware のゲスト Windows（自動ログオン済みのデスクトップ）で WinRemap を動かし、GUI とリマップ動作を自動で検証する。UI 自動化は画面のある対話セッション（session 1）でしか動かず、常駐アプリとグローバルフックを毎回きれいに消す必要があるため、VM とスナップショットを使う。

**判定に LLM は入らない。** ゲスト側の PowerShell が [Windows App Development CLI（`winapp`）](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/)か素の UI Automation クライアントでアプリを操作し、自分で合否を出す。v0.4〜v0.6 は Claude Code ＋ terminator MCP に英語のプロンプトを読ませる方式だったが、v0.7 で 5 本すべてを移植して廃止した。理由と実測は [ADR 0064](v0.7/decisions/0064-winapp-cli-for-ui-tests.md)。要点は 1 つで、**エージェントが「アプリの不具合」と報告した 3 件がすべて実測で否定された** — 誤るだけでなく、誤りの向きが「アプリのせい」に偏っていた。

実行の入口はこのリポジトリの `tests/ui/run-vm-ui-test.ps1`（ホスト側で実行）。ゲストを元に戻す部分は windows-utility の `test-vm/scripts/run-in-vm-vmware.ps1` に委譲する。

```
run-vm-ui-test.ps1（本リポジトリ）
  ├─ run-in-vm-vmware.ps1 -ConfigPath <本リポジトリの .secrets> -Restore
  │                                          … ready へ戻す→起動→実行可能まで待つ
  ├─ cargo build --release [--features test-inject]
  ├─ vmrun copyFileFromHostToGuest          … exe・fixtures\*.toml・guest\*.ps1 を C:\Test へ
  ├─ vmrun runProgramInGuest -interactive   … 検査スクリプトを session 1 で実行
  └─ 結果ファイルを回収し RESULT 行で集計 → revert
```

**被テストアプリの取得・ビルドは本リポジトリの仕事である。** 共通基盤は環境だけを用意する。`cargo build` とゲストへの配置はランナーが行う。

### ファイルの置き場所

| 場所 | 中身 |
|---|---|
| `tests/ui/run-vm-ui-test.ps1` | ホスト側の入口。検査の一覧（`$checks`）はここ |
| `tests/ui/fixtures/*.toml` | 検査用の設定ファイル。利用者向けの例ではない |
| `tests/ui/guest/<検査名>.ps1` | 検査の本体。**ファイル名は検査名と同じ** |
| `tests/ui/guest/ui-helpers.ps1` | 素の UIA クライアント・Win32・トレイメニュー |
| `tests/ui/guest/winapp-helpers.ps1` | winapp CLI のラッパー |
| `tests/ui/guest/promote-tray-icon.ps1` | トレイアイコンをタスクバーへ昇格させる（ランナーが呼ぶ） |
| `tests/ui/guest/probe-winapp.ps1` | 移行判断のための測定（[検証計画 §3](v0.5/notes/20260727_winapp-cli-migration.md)）。検査ではない |

ゲスト側は `C:\Test` に**平らに**展開される。検査スクリプトはヘルパーを `"$PSScriptRoot\ui-helpers.ps1"` で読み込むので、1 つのディレクトリに揃っている必要がある。

## 2. 前提

| 項目 | 値 |
|---|---|
| ホスト | VMware Workstation（Hyper-V の合成ディスプレイは OpenGL 非対応で設定画面が出ない） |
| ゲスト VM | `winremap-test`（`C:\VMware\winremap-test\winremap-test.vmx`）。**本テストスイート専用** |
| 接続情報 | 本リポジトリの `.secrets/test-vm.json`（`.gitignore` 済み） |
| ゴールデンスナップショット | `ready`（ツール導入・winapp CLI 込み。電源オフで取得） |
| ゲスト内 | winapp CLI `0.5.0`（`%LOCALAPPDATA%\Microsoft\WindowsApps`）、Windows PowerShell 5.1 |

**ゲストには Claude Code と terminator MCP がまだ入っているが、このリポジトリからは呼ばれない。** 撤去とゴールデンの再取得は windows-utility 側の作業で、[ADR 0064 §5](v0.7/decisions/0064-winapp-cli-for-ui-tests.md) が提案としてオーナー確認待ちである（撤去できれば、常駐 MCP がスナップショットに焼き付く問題と、ゲストに OAuth トークンを置く問題が構造的に消える）。

### 1 プロジェクト 1 VM

共通基盤（windows-utility）の `.secrets/test-vm.json` は**複数プロジェクトが使う既定スロット**である。実行のたびに書き換えると別プロジェクトのコマンドが意図しない VM に飛ぶ（2026-07-26 に実際に事故が起きた）。したがって:

- 接続情報は**本リポジトリの `.secrets/`** に置き、共通基盤側の `.secrets` は読み書きしない
- 対象 VM は**必ず `-ConfigPath` で明示的に渡す**
- `.secrets/` にはゲストのパスワードが平文で入る。**コミットしない・出力しない**（`.gitignore` に `/.secrets/` を追加済み。スクリプトも引数をエコーしない）

VM は `template-win11` の複製で作る（約 4 分。`setup-01`＋`setup-02` の約 30 分が不要）。作り直すときも同じ:

```powershell
cd D:\Projects\GitLab\windows-utility\test-vm\scripts
$cfg = "D:\Projects\GitHub\winremap\.secrets\test-vm.json"

.\clone-vm-vmware.ps1 -NewVMName winremap-test -Snapshot ready `
    -SourceConfigPath ..\..\.secrets\test-vm.template-win11.json -NewConfigPath $cfg

.\snapshot-golden-vmware.ps1 -ConfigPath $cfg -AppProcessNames winremap
```

**`-AppProcessNames winremap` を省かないこと。** ゴールデンに常駐したままの WinRemap は revert のたびに復活し、ログファイルを掴んだまま次の実行を**無言でタイムアウト**させる。ゴールデンを取り直すときは毎回付ける。

## 3. 実行方法

```powershell
cd D:\Projects\GitHub\winremap\tests\ui
.\run-vm-ui-test.ps1                              # 全検査
.\run-vm-ui-test.ps1 -Check 05-remap-notepad      # 1 本だけ
.\run-vm-ui-test.ps1 -Check 01-settings-window,04-log-window
```

| オプション | 既定 | 用途 |
|---|---|---|
| `-Check` | `all` | 検査名。カンマ区切りで複数指定できる。`-Scenario` も同じ意味で受け付ける（旧名。過去の受け入れチェックリストがその名前で書かれている） |
| `-EntryScript` | windows-utility の既定パス | 別マシンでリポジトリの場所が違う場合に指定 |
| `-VmConfig` | `test-vm.json` | 本リポジトリの `.secrets\` 配下のファイル名、またはフルパス |
| `-Snapshot` | `ready` | 戻す先のスナップショット |
| `-NoRevert` | — | revert せず今のゲストで実行（**検査を書いている間だけ**。再現性は失われる） |
| `-SkipBuild` | — | ビルド済みバイナリを再利用 |
| `-DumpUia` | — | 検査を走らせず UIA ツリーを出す（§7） |

**検査は 1 本ごとに revert から始まる。** 前の検査が残した常駐プロセス・フック・レジストリを持ち越さないためで、通しの所要時間の大半はこの revert（電源オフから約 1 分）である。

終了コードは全検査 PASS で 0、1 つでも FAIL・SETUP FAILED・ERROR なら 1。存在しない検査名を渡した場合は既知の名前を並べて止まる。

## 4. 検査の一覧

| 検査 | 中身 | 由来する受け入れ項目 |
|---|---|---|
| `00-uia-actuation` | 設定・ログウィンドウが UIA に出て、`Edit`・`Clear` が押せる。**素の UIA クライアントで書かれた唯一の検査**（§5） | v0.2 B0-2・B1-1・B1-2 |
| `00-cli-smoke` | `--version` / `--help` / リダイレクト / 壊れた設定と不明な引数のダイアログ / 無音起動 | v0.1 起動スモーク、v0.2 A-1・A-4〜A-6・A-8・A-9・B0-11 |
| `00-regression` | ウィンドウのライフサイクル、操作ログ、複数キーマップの表示、`--lang ja` | v0.2 A-15〜A-19・B0-5〜B0-12、v0.3 M-1・M-63・M-64・M-70・M-82 ほか |
| `00-log-view` | ログの 2 モード（簡易／全イベント）、時刻・タグの列、クリップボード（[ADR 0057](v0.5/decisions/0057-log-view-modes.md)）、キー名・前面アプリ行・操作の記録・`--debug` の有無によるコンソール出力（[ADR 0058](v0.5/decisions/0058-log-readability.md)）、制御コード表示と印字可能文字を出さないこと（[ADR 0056](v0.5/decisions/0056-control-codes-in-the-log.md)） | v0.5 |
| `01-settings-window` | 設定ウィンドウが開き、`Edit`・`General`・`Keymaps`・`notepad`・バージョン行が出ていること | v0.4 A-1 の一部、v0.2 B0-2 |
| `02-config-display` | アドレスバーのフォルダーとコンボボックスの値が `C:\Test` / `minimal.toml`、ナビに `General`・`Keymaps`・`notepad`、キーマップを選ぶと `notepad.exe`・`C-h`・`Back` が出ること | v0.2 B1-1・B1-2 |
| `03-tray-actions` | 有効/無効の往復・Reload・Quit。判定は**トレイアイコンの名前**（§6） | v0.2 A-21 |
| `04-log-window` | セッション行・`Follow newest`・`Clear`・`Copy all` | v0.2 A-10・B1-29 |
| `05-remap-notepad` | Notepad で `C-h` → `x` が入り、置換パネルが開かないこと。**さらに WinRemap を止めて同じキーを送り、パネルが開くことまで確かめる** | v0.1 M2-1 と同じ経路 |
| `06-foreground-line` | ログウィンドウを開いたまま前面アプリを切り替え、①前面アプリ行が切り替えた先を名指しするか ②**そのアプリに紐付いたルールが選ばれ続けるか**。**ウィンドウを 1 枚も開かない対照**と対で測る（§9） | v0.5 からの持ち越し、v0.7 開発計画 §3.5 |

`01`〜`05` は v0.6 までエージェント版シナリオだったもので、**名前も対象も同じまま中身を PowerShell にした**。仕分けの全体像（どの項目を自動で通し、何を手で残すか）は [v0.5 受け入れチェックリスト](v0.5/03_acceptance-checklist.md) が正である。

`06-foreground-line` は機能を網羅するための検査ではなく、**持ち越した 1 つの観測に決着を付けるための検査**である（§9）。前半（行が出るか）はキーを押さないが、**後半でコマンドを 1 つ押す** — 前面アプリ行を書く関数が、フックがキーマップを選ぶときに読むキャッシュも更新しているためで、**「行が出ない」が表示だけの話かどうかは打ってみないと分からない**。

表に載っていない検査が 1 つある。**`90-probe-foreground`（診断）** は、`06-foreground-line` が見つけた不具合が**どこで壊れているか**を切り分けるための道具である。同じ切り替えを 4 つの条件で行い、**別プロセスの独立した `EVENT_SYSTEM_FOREGROUND` クライアント**（`foreground-listener.ps1`）と WinRemap の記録を突き合わせる。`-Check 90-probe-foreground` と名指ししたときだけ動き、**`all` には入らない** — 新しい退行を見張る検査ではなく、既知の不具合を追い込む道具だからである。**残してあるのは、追い込み方そのものが記録だからである。**

`00-log-view`・`05-remap-notepad`・`06-foreground-line` は **test-inject ビルド**で動く（§8）。`00-log-view` は**両モードのスクリーンショットを `%TEMP%\winremap-log-view-*.png` に残す**。読みやすさは表明で確かめられないので、そこは人が見る。

各検査は結果ファイル（`%TEMP%\winremap-<検査名>.txt`）に `CHECK` 行・`RESULT` 行・採取したツリーを残す。**判定はその集計行が正**で、ランナーはそれを読むだけである。vmrun はゲストの標準出力を持ち帰らないので、ファイルに残さない検査は「黙って死んだ」と「そもそも走らなかった」の区別が付かない。

## 5. 検査の書き方

### 素の UIA 実装（`00-uia-actuation`）は消さない

`01`・`02` と見ている対象が重なるが、**これだけは winapp を通さない**。「winapp が間違っている」と「アプリが間違っている」を切り分けられる実装が他に無いためである。移行中に 2 度、**こちら側のバグが「アプリの不具合」の形で報告された**（[ADR 0064 §3](v0.7/decisions/0064-winapp-cli-for-ui-tests.md)）。独立した第 2 の実装が同じウィンドウを見て「ある」と言えば、疑う先が 1 手で決まる。

### winapp CLI の作法（すべて実測済み。help には書かれていない）

| 決まり | 理由 |
|---|---|
| **読むのは `--json` だけ** | テキスト出力は PowerShell 5.1 で CP932 として読まれ文字化けする。JSON の `\uXXXX` は無事 |
| **引数は配列で渡す** | `ValueFromRemainingArguments` に配列を渡すと 1 本の文字列に潰れ、winapp は usage を出す |
| **`ConvertFrom-Json` の結果はいったん変数に受ける** | パイプラインのまま返すと配列が 1 要素のままになる |
| **一致が 1 件の関数の戻り値は `return , $hits`** | PowerShell 5.1 は 1 要素の配列を剥がし、`PSCustomObject` に `Count` メンバーが無い。`$hits.Count -eq 1` が空と比較され、**要素があるのに「無い」と読まれる**。0 件のときだけ正しく動くので気づきにくい |
| **`wait-for` はセレクター必須。`--timeout` はミリ秒** | 秒で書くと 15 ミリ秒待って「無い」と言う |
| **`wait-for` は名前が 2 要素に一致すると「見つからない」と言う** | 一意な名前を使うか、`search --json` でスラッグに解決してから渡す |
| **`wait-for` はアプリの起動を待たない。`--gone` で終了も判定できない** | 起動待ち・終了待ちは `Get-Process` |
| **`list-windows` は該当ゼロでも終了コード 0** | 件数か `--json` を見る |
| **`get-property --json` は `properties` を 1 段下げ、名前を大文字で返す** | `inspect` の形と違う。コンボボックスが**表示している値**はこの経路でしか読めない（`name` は空） |
| **スラッグ（`btn-...-7753`）は実行ごとに変わる** | ハードコードしない。名前で引き、必要なら `search` で解決する |
| **昇格したトレイアイコンは `btn-systemtrayicon-*` セレクタを持たない** | セレクタで絞ると、アイコンがツリーにいるのに「無い」と報告する |
| **`inspect` はコンテナの名前として、配下の名前を連結して返す** | 無効化直後の通知領域は `WinRemap — 1 keymap(s) WinRemap (disabled)` と読める。**最短一致**を採るか、部分一致だけで判定する |

### キーを送るときは、押されたことを効果で確かめる

| 事実 | 影響 |
|---|---|
| 和音の綴りは **`ctrl+h`**。`^h` と `{Ctrl}h` は**終了コード 0 のまま「文字として」入る** | 綴りを間違えても winapp は成功を返す。**送出のたびに対象のテキストを読み直す** |
| `--via send-input` が必須。既定の `--via post-message` はウィンドウメッセージで、**低レベルフックを通らない** | 既定のままでは何も検証していないことになる |
| **固定 sleep は判定ではなく「記録」を壊す** | 800 ms 後に 1 回読む実装では、`abc` の 3 文字目が間に合わない実行があり、winapp で入ったのに `keybd_event` で入ったと記録していた。**期待する文字列になるまで読み直す** |

### 決定論を保つ

- **要素の名前は推測せず `-DumpUia` で採取する**（§7）。GUI を変えたら流し直して検査を直す
- **新しい判定を足したら 2 回通して一致を確かめる。** 移植の完了条件もこれだった
- **負例を必ず入れる。** 「成功するはずのものが成功した」だけでは、表明が効いているのか通ったのか区別できない。`05-remap-notepad` は WinRemap を止めて同じキーを送り、**置換パネルが開くことまで**確かめる — 開かなかったことに意味があるのは、開くときに見えると分かっている場合だけである

## 6. トレイの扱い

- **通知領域のオーバーフロー（∧）は UI 自動化では開けない。** そこで**検査の前にアイコンをタスクバー上へ出す**（`promote-tray-icon.ps1` が `HKCU\Control Panel\NotifyIconSettings` の `IsPromoted` を立てる）。ランナーが `NeedsTray` の検査の前に自動実行し、`promoted=1` を確認できなければ検査を走らせず `SETUP FAILED` とする
- **メニューを開くのは素の Win32 の右クリック、項目を選ぶのは winapp の `invoke <コマンド ID>`。** winapp が可視トレイ（`Shell_TrayWnd`）を押せないのは「前面でないウィンドウは押さない」という**winapp の安全弁**であって Windows の制限ではなく、合成マウスイベントにその制限は無い
  - コマンド ID: `1001` 有効/無効・`1002` 再読み込み・`1003` 設定・`1004` ログを表示・`1005` 終了
  - **頭文字キー方式は使わない。** 表示言語に依存し、`--lang ja` では成立しない
- **トレイの右クリックメニューは UIA に出たり出なかったりする。** Win32 のポップアップ（クラス `#32768`）で、**中身のない `Pane`** にしか見えないことがある。**AccessKit とは無関係で、Windows のメニューの性質である**。だから**メニューは操作にだけ使い、判定はメニュー以外で行う** — `03-tray-actions` は有効/無効を**トレイアイコンの名前**（ツールチップ）で判定する。アイコン自体は常にツリーに出る

## 7. セレクタは推測せず採取する

```powershell
.\run-vm-ui-test.ps1 -DumpUia
```

検査を走らせず、設定ウィンドウとログウィンドウの UIA ツリー（role・name・対応パターン・値）をそのまま出す（`tests/ui/guest/dump-uia.ps1`）。**GUI を変えたら流し直し、検査が探す名前を合わせる。**

### 設定・ログウィンドウの中身（v0.5.0 で解決済み）

**v0.4 まではスクリーンショットを撮って AI に読ませていた。v0.5.0 からは UIA で読めて押せる。**

- 見えなかった理由は「OpenGL で描いているから」**ではない**（当初そう書いていたが誤り。オーナー指摘 2026-07-26）。UIA への露出は描画方式と無関係で、egui は [AccessKit](https://accesskit.dev/) 経由でアクセシビリティツリーを出せる。原因は次の 2 点だった
  1. `Cargo.toml` が eframe を `default-features = false` で取っており、**既定機能である `accesskit` が外れていた**
  2. 有効にしても足りない。**eframe 0.35 は AccessKit アダプターを ROOT ビューポートにしか作らない**。WinRemap の root は不可視 1×1 のホストで、設定・ログは遅延子ビューポート（[ADR 0037](v0.2/decisions/0037-gui-invisible-host-viewport.md)）なので、実ウィンドウにはアダプターが付かなかった
- 対処は [ADR 0055](v0.5/decisions/0055-accesskit-for-child-viewports.md)。eframe をフォークして**ウィンドウを持つ全ビューポートにアダプターを付け**、`[patch.crates-io]` で暫定的に指している（上流にマージされたら外す）
- 実測結果は[調査記録](v0.5/notes/20260726_accesskit-child-viewport.md)。設定ウィンドウ 43 要素（キーマップ選択で 59、編集モードで 72）、ログウィンドウ 7 要素

## 8. リマップ検証だけが特別な理由

WinRemap は自己送出ループを防ぐため、注入イベント（`LLKHF_INJECTED`）を素通しする（AGENTS.md 不変条件 1）。**スクリプトからキーを押す方法はすべて注入である**（`keybd_event` も `SendInput` も winapp の `send-keys --via send-input` も）。したがって通常のビルドではテストのキー送出はリマップされない。

そこで、既定 OFF の Cargo feature `test-inject` を有効にしたビルドでのみ `--accept-injected` を受け付け、**自分が注入したイベント（`dwExtraInfo` のマーカー付き）以外**の注入を変換対象にする（[ADR 0053](v0.4/decisions/0053-test-inject-mode.md)）。自分の注入は従来どおり無条件で素通しするので、自己送出ループは閉じたままである。

- ランナーは `$checks` の `NeedsInject` が立っている検査を `--features test-inject` でビルドする。それ以外は出荷物と同じ形のビルドで実行する
- **配布物にはこのコードが入らない。** `cargo build --release`（feature 無し）ではフラグ自体が存在せず、`--accept-injected` は不明な引数として拒否される（`src/main.rs` の単体テストで担保）
- このモードで起動した実行ファイルは、起動ログとトレイメニューの先頭行に `TEST BUILD` を出す

起動は必ず `--config <パス> --lang en` を付ける。**`--lang en` はゲストのロケール（日本語）に関係なく UI 文字列を英語に固定する**ためで、これが無いと検査が探す名前が環境依存になる。

`05-remap-notepad` だけは `examples\minimal.toml` ではなく `tests/ui/fixtures/uitest.toml`（`C-h` → `x`）を使う。理由はそのファイルの先頭に書いてあるが、要点は**判定を「文字が消える」ではなく「文字が入る」に置く**ためである（§9）。

## 9. 期待結果の置き方（実際に踏んだ落とし穴）

- **「入る」で判定し、「消える」で判定しない。** セレクタ経由でキーを送ると**キャレットが文書の先頭へ移動**するため、先頭での Backspace は何も消さない。`C-h` → `Back` を期待結果にすると、リマップが成功していても文字数が変わらず落ちる。挿入（`C-h` → `x`）なら観測が一意になる
- **最新の Notepad の検索/置換は別ウィンドウではなくウィンドウ内パネル**である。ウィンドウ名の一覧だけで「ダイアログは出ていない」と判定すると誤る。Notepad の UI ツリーの中を見る（`05-remap-notepad` は**押す前後の名前の差分**を取り、そこに検索/置換の語が現れるかで判定する。もとから居るメニュー項目を拾わずに済む）
- **ハーネスの挙動とアプリの挙動を混ぜない。** 上の 2 点はどちらも「WinRemap のバグに見えるテスト側の性質」だった。疑わしいときは `00-uia-actuation` と `--debug` 出力で先に切り分ける
- **WinRemap 自身のウィンドウが前面にあるときに注入したキーは、WinRemap のフックに届かない**（2026-07-28 測定、`00-log-view` の作成中に判明）。**キーは必ず他のアプリ（Notepad）に対して送ること。**
  - 測定内容: ログウィンドウを前面にして `keybd_event`・`SendInput`（vk）・`SendInput`（スキャンコード）の 3 通りを送ると**ログに 1 行も出ない**。同じスクリプトの同じ関数で、Notepad を前面にして送ると**全部出る**。ウィンドウを 1 枚も開かず `--debug` の出力をファイルに落とした場合も**全部出る**（この 3 例が `00-log-view` の実行記録に残っている）
  - **アプリの欠陥か、注入の性質かは未確定。** ログウィンドウが開いているだけなら問題は起きず、**前面にあるとき**だけ起きる。人が実キーボードで打つ経路とは別なので受け入れは止めないが、**H-2（止まらない・遅れない）を通すときにログウィンドウを前面にした状態も見ること**
- **ログウィンドウを開いている間、切り替えた先のアプリが前面アプリ行に出ない**（2026-07-29、`00-log-view` に前面アプリ行の検査を足したときに 2 回の実行で観測）。報告されたのは `winremap.exe` と `explorer.exe` だけで、**Notepad は一度も出なかった** — `GetForegroundWindow()` で前面であることを確認し、そのキーがフックに届いている（判定行が出ている）にもかかわらず、である
  - **テスト側の対処**: 前面アプリ行の検査は**どのアプリが出たか**ではなく**行の中身**（`application` の値とキーマップ一覧が揃っているか）で判定する。実際に出たアプリ名は `F|` 行として実行記録に残す
  - **ウィンドウの有無で切り分け済み（3 回目の実行）**。**WinRemap のウィンドウを 1 枚も開かず `--debug` だけで動かすと、Notepad への切り替えは報告される**:

    ```
    07:19:47.368 [window] application = "notepad.exe" — matching keymaps: global
    07:19:47.368 [window]   C:\Program Files\WindowsApps\Microsoft.WindowsNotepad_...\Notepad.exe
    ```

    つまり**ログウィンドウが開いているときだけ**起きる。この切り分け記録は `00-log-view` の実行記録に `W|` 行として毎回残る
  - **2026-08-01 に決着した。`06-foreground-line` が症状を再現し、`90-probe-foreground` が原因を突き止めた（[調査ノート](v0.7/notes/20260801_foreground-race.md)）。**
  - **原因は競合状態で、ウィンドウの有無は関係ない。** 同じイベントを購読する独立したクライアントを別プロセスで動かして 1 対 1 で突き合わせると、**イベントは 18 件すべて届いており、4 件（22%）で WinRemap が「直前まで前面だったウィンドウ」を答えていた**。`src/window.rs` がイベントの `hwnd` を捨てて `GetForegroundWindow()` を引き直しているためで、切り替えが確定する前に引くと前の値が返る
  - **この節の上に書いてあった「ログウィンドウが開いているときだけ起きる」は誤りだった。** 4 段階の測定のうち**対照（ウィンドウ無し）でも起きた**。**発生率 22% の事象を条件ごとに数回ずつ観測すると、条件と相関しているように見える** — これは測り方の教訓であって、アプリの性質ではない
  - **v0.5 の「exe キャッシュ自体は追従している」という記述は取り消す。** `05-remap-notepad` が通るのは**当たらなかったから**で、追従の証拠にはなっていなかった
  - **2026-08-01、修正した**（[ADR 0065](v0.7/decisions/0065-foreground-window-from-the-event.md)）。同じスクリプト・同じ回数で測り直して **24 回中 2 件 → 0 件**
  - **検査側への申し送り**: `06-foreground-line` は確率的な事象を 1 回の切り替えで判定しているので**壊れていても緑になりうる**。**この不具合の回帰判定は `90-probe-foreground` の食い違い数で行うこと**
- **ログウィンドウには「どの設定ファイルを読み込んだか」が出ない。** 起動時の `N keymap(s) loaded from <path>` は `notify::console_line`（`src/main.rs`）にしか流れず、ログウィンドウへは渡っていない。一方でトレイからの再読み込みは `reload_ok` をログウィンドウに出す（`src/tray.rs`）。この非対称をどうするかはオーナー判断待ちで、検査は現在の実挙動（起動バナーが読めること）を検証する

## 10. 実行記録

| 日付 | 内容 |
|---|---|
| 2026-07-25 | 1 コマンドで全 5 シナリオ（エージェント版）が PASS。通し 25 分前後 |
| 2026-07-27 | 専用 VM `winremap-test` へ載せ替え。`-DumpUia` 5/5、revert は電源オフから 58 秒 |
| 2026-07-28 | **エージェント版を 2 回通して 6 件とも一致**（下表）。ハーネス検査 `00-cli-smoke`・`00-regression` を追加し全 8 件 PASS（通し約 40 分） |
| 2026-07-29 | `00-log-view` を追加（26 チェック）。v0.5.0 のリリース受け入れを全 9 件 PASS で通した |
| 2026-07-31 | **winapp CLI へ移行**。01 を 5 回連続で 12/12 一致（各 17 秒）、02〜04 を 2 回通して 46 チェック一致（4 本計 1 分 27 秒）、05 を 2 回一致（各 30 秒）。エージェント版を廃止（[ADR 0064](v0.7/decisions/0064-winapp-cli-for-ui-tests.md)） |
| 2026-08-01 | **修正の確認**。`hwnd` を使う形に直し（[ADR 0065](v0.7/decisions/0065-foreground-window-from-the-event.md)）、**同じスクリプトのまま**測り直して食い違い **2/24 → 0/24**。アプリ別キーマップも打鍵で確認 |
| 2026-08-01 | **原因調査**。`90-probe-foreground` と `foreground-listener.ps1` を追加し、独立したクライアントとの突き合わせで**イベントは全件届いており 22% が誤った名前を答えている**ことを測った（[調査ノート](v0.7/notes/20260801_foreground-race.md)）。**「ログウィンドウが開いているときだけ」という前日の見立ては誤りだった** |
| 2026-08-01 | `06-foreground-line` を追加（12 チェック）。**v0.5 からの持ち越しに決着**（§9）。3 件 FAIL で、それが正しい判定である — アプリ側の未解決事項として[受け入れチェックリスト §6.1](v0.7/03_acceptance-checklist.md) に上げた。書いている途中で**こちら側のバグを 2 回踏んだ**（呼び出し元スコープの `$lines` を仮引数 `$Lines` が隠して `Say` が壊れた件、`return , $x` を `@()` で受けて全行が 1 要素になった件）。どちらも最初は「アプリが報告しない」形で現れた |
| 2026-07-31 | 構成を作り直したあとの**通し実行で全 9 検査 PASS**。1 件だけ落ちた `00-log-view` の `the-oem-key-is-named` は**アプリではなく表明が古かった** — v0.7 の [ADR 0063 §5](v0.7/decisions/0063-symbol-keys.md) が `/ (0xBF)` の括弧を外しており、括弧を要求する v0.5 の表明が残っていた。表明を新しい規則へ書き換えて 26/26。**テストの表明も「前バージョンの記述」である** |

移行前後の比較（同じ判定を出すのにかかった時間）:

| 検査 | エージェント版（2026-07-28、2 回） | 現行 |
|---|---|---|
| `01-settings-window` | 1:39 / 1:06 | **0:17** |
| `02-config-display` | 2:29 / 2:21 | **0:16** |
| `03-tray-actions` | 3:18 / 3:26 | **0:40** |
| `04-log-window` | 1:22 / 1:59 | **0:14** |
| `05-remap-notepad` | 3:34 / 1:19 | **0:30** |

**エージェント版は同じ判定でも 1〜2 分ぶれていた**（プロンプトは同一で、ぶれていたのは実行時間であって結論ではない）。現行は 01 の 5 回連続実行で、判定だけでなく**採取した UIA ツリーの差分がステータスバーの `Started: <時刻>` 1 行だけ**だった。

## 11. ハマりどころ

| 事象 | 対処 |
|---|---|
| **revert したのに前回の実行が生き返る／ゲストログが 0 バイトでタイムアウト** | ゴールデンスナップショットが**実行中（メモリ状態込み）で取得**されていると、revert のたびに当時のプロセスが復活し、ログファイルを掴んだまま新しい実行を沈黙させる。根治は `snapshot-golden-vmware.ps1`（電源オフで取得。`-AppProcessNames winremap` 必須）で取り直すこと。§2 参照 |
| **別プロジェクトのテストが知らない VM に飛ぶ** | 共通基盤の `.secrets/test-vm.json` を書き換える運用をやめ、`-ConfigPath` で明示する方式に統一した（§2） |
| **revert 直後に検査が「要素が無い」で落ちる** | VMware Tools が資格情報に応答してから、デスクトップが対話プログラムを受け付けるまでには数分の開きがある。固定時間の `Start-Sleep` で埋めてはいけない。ランナーは `run-in-vm-vmware.ps1 -Restore` に委譲し、**実行可能になるまでの待ち**を基盤側の 1 箇所に集約している |
| **検査が「結果ファイルが無い」で ERROR になる** | ゲストでスクリプトが落ちたか、そもそも配置されていない。ペイロードは `guest\*.ps1` を**ディレクトリごと**送るので後者は起きにくいが、起きたら `C:\Test` の中身を見る |
| 設定画面が黒い・出ない | Hyper-V では OpenGL が無い。VMware で実行する |
| 日本語が文字化けする | ゲストの PowerShell 5.1 は BOM 無し UTF-8 を CP932 として読む。**検査スクリプトは ASCII で書き**、日本語の判定値はコードポイントから組み立てる（`05-remap-notepad` の検索/置換の語がその例） |
| UI 自動化が無反応 | session 1（対話デスクトップ）でしか動かない。ランナー経由で実行する |
| ビルドが `アクセスが拒否されました` で失敗 | ホストで `target\ui-release\...\winremap.exe` が起動中。終了させる（`-SkipBuild` で回避も可） |
| **`reverting to snapshot` から進まない（ゲストは正常に起動している）** | `vmrun start` は VMware Workstation の GUI を起こし、その GUI は vmrun より長く生き残る。**vmrun 自身の終了より多くを待つ書き方**をすると、実質「利用者が VMware を閉じるまで」待つ。`& vmrun ... \| Out-Null` は stdout パイプの EOF を待ち、`Start-Process -Wait` は**プロセスとその子孫全部**を待つ。ランナーはどちらも避けて `Invoke-VmrunHost` で 1 プロセスだけを待つ |

## 12. やらないこと

- **CI（GitHub Actions）への組み込み**: VMware とゲストの認証情報がホスト固有であり、手元実行に留める
- **キー入力内容のログ収集**: テストのためであってもキーロガー化はしない（不変条件 6）
- **`--accept-injected` の配布物への露出**: 利用者向けドキュメント（README・ヘルプサイト）には載せない
