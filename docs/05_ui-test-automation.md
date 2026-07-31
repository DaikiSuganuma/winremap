# UI テスト自動化（VM ＋ AI エージェント）

> 元資料: windows-utility リポジトリの `test-vm/利用プロジェクト向けガイド.md`（利用側が守ること・書くこと）と
> `test-vm/セットアップ手順.md`（構築・設計判断）。
> 環境の構築・保守はそちらが正であり、本書は**このリポジトリ側の使い方**を記述する。
> **何をテストするかは本リポジトリ側に置く**（被テストアプリの取得・ビルド・シナリオを共通基盤に置かない）のが基盤側の方針である。

- 作成日: 2026-07-25 ／ 更新日: 2026-07-27（共通基盤の「1 プロジェクト 1 VM ／ `-ConfigPath` で明示」運用へ追随）
- 作成: Claude Code（AI モデル: claude-opus-5）／レビュー・承認: オーナー
- 関連: [ADR 0053](v0.4/decisions/0053-test-inject-mode.md)（注入イベント受理モード）、[v0.4 開発計画 §6](v0.4/01_development-plan.md)（Phase E）、[winapp CLI 移行検討](v0.5/notes/20260727_winapp-cli-migration.md)

---

## 1. これは何か

VMware のゲスト Windows（自動ログオン済みのデスクトップ）で Claude Code ＋ Terminator MCP（Model Context Protocol: AI が外部ツールを呼ぶための規格）に画面を操作させ、WinRemap の GUI とリマップ動作を自動で検証する。UI 自動化は画面のある対話セッション（session 1）でしか動かず、常駐アプリとグローバルフックを毎回きれいに消す必要があるため、VM とスナップショットを使う。

実行の入口はこのリポジトリの `tests/ui/run-vm-ui-test.ps1`（ホスト側で実行）。ゲストへコマンドを流す部分は windows-utility の `test-vm/scripts/run-in-vm-vmware.ps1` に委譲する。

```
run-vm-ui-test.ps1（本リポジトリ）
  ├─ run-in-vm-vmware.ps1 -ConfigPath <本リポジトリの .secrets> -Restore
  │                                          … ready へ戻す→起動→実行可能まで待つ
  ├─ cargo build --release [--features test-inject]
  ├─ vmrun copyFileFromHostToGuest          … winremap.exe と minimal.toml を C:\Test へ
  ├─ run-in-vm-vmware.ps1 -ConfigPath … -Command "claude -p <シナリオ>"
  │    └─ ゲスト session 1 で Terminator MCP が UI を操作
  └─ 最終行の PASS / FAIL を集計 → revert
```

**被テストアプリの取得・ビルドは本リポジトリの仕事である。** 共通基盤は環境だけを用意する（`-SetupWinRemap` は基盤側から削除された）。`cargo build` とゲストへの配置はランナーが行う。

## 2. 前提

| 項目 | 値 |
|---|---|
| ホスト | VMware Workstation（Hyper-V の合成ディスプレイは OpenGL 非対応で設定画面が出ない） |
| ゲスト VM | `winremap-test`（`C:\VMware\winremap-test\winremap-test.vmx`）。**本テストスイート専用** |
| 接続情報 | 本リポジトリの `.secrets/test-vm.json`（`.gitignore` 済み） |
| ゴールデンスナップショット | `ready`（ツール導入・認証・Terminator・winapp CLI 込み。電源オフで取得） |
| ゲスト内 | Claude Code（`CLAUDE_CODE_OAUTH_TOKEN` を User 環境変数に）、Terminator MCP `0.24.28` |

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
.\run-vm-ui-test.ps1                              # 全シナリオ
.\run-vm-ui-test.ps1 -Scenario 05-remap-notepad   # 1 本だけ
```

| オプション | 既定 | 用途 |
|---|---|---|
| `-Scenario` | `all` | `scenarios\` のファイル名（拡張子なし）、または `00-cli-smoke` などハーネス検査の名前。カンマ区切りで複数指定できる |
| `-EntryScript` | windows-utility の既定パス | 別マシンでリポジトリの場所が違う場合に指定 |
| `-VmConfig` | `test-vm.json` | 本リポジトリの `.secrets\` 配下のファイル名、またはフルパス |
| `-Snapshot` | `ready` | 戻す先のスナップショット |
| `-TimeoutMin` | `25` | 1 シナリオあたりの上限（シナリオ側で `# timeout: <分>` を指定可） |
| `-NoRevert` | — | revert せず今のゲストで実行（**プロンプト調整中のみ**。再現性は失われる） |
| `-SkipBuild` | — | ビルド済みバイナリを再利用 |

終了コードは全シナリオ PASS で 0、1 つでも FAIL または判定不能なら 1。

## 4. シナリオ

`tests/ui/scenarios/*.txt` が 1 ファイル 1 シナリオで、中身はそのまま `claude -p` に渡す英語プロンプトである。**最終行に `PASS` か `FAIL` だけを出す**よう指示してあり、ランナーはそれを拾う。

シナリオ（エージェントが読む）とは別に、**AI を挟まないハーネス検査**が `tests/ui/guest/*.ps1` にある。`all` の実行では**シナリオより先に**流れ、名前を指定して単体でも回せる。

| 検査 | 中身 | 由来する受け入れ項目 |
|---|---|---|
| `00-uia-actuation` | 設定・ログウィンドウが UIA に出て、`Edit`・`Clear` が押せる | v0.2 B0-2・B1-1・B1-2 |
| `00-cli-smoke` | `--version` / `--help` / リダイレクト / 壊れた設定と不明な引数のダイアログ / 無音起動 | v0.1 起動スモーク、v0.2 A-1・A-4〜A-6・A-8・A-9・B0-11 |
| `00-regression` | ウィンドウのライフサイクル、操作ログ、複数キーマップの表示、`--lang ja` | v0.2 A-15〜A-19・B0-5〜B0-12、v0.3 M-1・M-63・M-64・M-70・M-82 ほか |
| `00-log-view` | ログの 2 モード（簡易／全イベント）、時刻・タグの列、クリップボード（[ADR 0057](v0.5/decisions/0057-log-view-modes.md)）、キー名・前面アプリ行・操作の記録・`--debug` の有無によるコンソール出力（[ADR 0058](v0.5/decisions/0058-log-readability.md)）、制御コード表示と印字可能文字を出さないこと（[ADR 0056](v0.5/decisions/0056-control-codes-in-the-log.md)） | v0.5 |
| `01-settings-winapp` | シナリオ `01-settings-window` を **winapp CLI で書き直したもの**（AI 無し）。設定ウィンドウが開き、`Edit`・`General`・`Keymaps`・`notepad`・バージョン行が出ていること | v0.4 A-1 の一部、v0.2 B0-2 |
| `02-config-winapp` | 同 `02-config-display`。アドレスバーのフォルダーとコンボボックスの値が `C:\Test` / `minimal.toml`、ナビに `General`・`Keymaps`・`notepad`、キーマップを選ぶと `notepad.exe`・`C-h`・`Back` が出ること | v0.2 B1-1・B1-2 |
| `03-tray-winapp` | 同 `03-tray-actions`。有効/無効の往復・Reload・Quit を**トレイアイコンの名前**で判定する | v0.2 A-21 |
| `04-log-winapp` | 同 `04-log-window`。セッション行・`Follow newest`・`Clear`・`Copy all` | v0.2 A-10・B1-29 |
| `05-remap-winapp` | 同 `05-remap-notepad`。Notepad で `C-h` → `x` が入り、置換パネルが開かないこと。**さらに WinRemap を止めて同じキーを送り、パネルが開くことまで確かめる** | v0.1 M2-1 と同じ経路 |

上の 5 本は **v0.7 の winapp CLI 移行（Phase 4 まで完了）**で作った、エージェント版と同じことを AI 無しで確かめる検査である。01〜04 は**2 回通して 46 チェックが全件一致**（4 本合計 1 分 27 秒。エージェント版は 8 分 48 秒）、05 は**10 チェックが 2 回一致**（各 30 秒。エージェント版は 3:34 / 1:19）。[v0.7 開発計画 §3.4.1・§3.4.2](v0.7/01_development-plan.md) が実測と踏んだ落とし穴の記録。terminator 版を消すかどうかは移行 Phase 5 の判断なので、**当面は並走**する。

`00-log-view` と `05-remap-winapp` は **test-inject ビルド**で動く（[ADR 0053](v0.4/decisions/0053-test-inject-mode.md)）。キーを合成して送る以上それは注入であり、出荷ビルドでは素通しされて判定行がそもそも出ないからである。`00-log-view` は**両モードのスクリーンショットを `%TEMP%\winremap-log-view-*.png` に残す**。読みやすさは表明で確かめられないので、そこは人が見る。

**キーを送るときは、押されたことを効果で確かめる。** `05-remap-winapp` の移植で実測したこと（[§3.4.2](v0.7/01_development-plan.md)）:

| 事実 | 影響 |
|---|---|
| 和音の綴りは **`ctrl+h`**。`^h` と `{Ctrl}h` は**終了コード 0 のまま「文字として」入る** | 綴りを間違えても winapp は成功を返す。送出のたびに対象のテキストを読み直す |
| `--via send-input` が必須。既定の `--via post-message` はウィンドウメッセージで、**低レベルフックを通らない** | WinRemap のフックに届かないので、既定のままでは何も検証していないことになる |

仕分けの全体像（どの項目を自動で通し、何を手で残すか）は [v0.5 受け入れチェックリスト](v0.5/03_acceptance-checklist.md) が正である。

| ファイル | 内容 | 対応する受け入れ項目 |
|---|---|---|
| `01-settings-window` | 設定画面が開き、要素が UIA に出て `Edit` が押せる | v0.4 A-1 の一部、v0.2 B0-2 |
| `02-config-display` | アドレスバーのパスとキーマップ表示が `minimal.toml` と一致する | v0.2 B1-1・B1-2 の最小構成 |
| `03-tray-actions` | 有効/無効トグル・Reload・Quit | v0.2 A-21 とトレイ操作 |
| `04-log-window` | ログ行・追従チェックボックス・`Clear`／`Copy all` | v0.2 A-10・B1-29 |
| `05-remap-notepad` | Notepad で `C-h` → `x`（文字が入り、置換パネルが開かない）。**トレイには触れない** | v0.1 M2-1 と同じ経路 |

どこまでを自動で済ませ、何が手動に残るかは [v0.4 受け入れチェックリスト](v0.4/03_acceptance-checklist.md) の §G が正である。

**現状（2026-07-25）**: **`.\run-vm-ui-test.ps1` の 1 コマンドで全 5 シナリオが PASS**（エージェント実行部分は 1 本あたり 2〜7 分。シナリオごとに revert・起動・配置が入るため通しでは 25 分前後）。ゴールデンスナップショットは同日に windows-utility 側で電源オフ状態へ取り直され、revert 直後に残骸プロセスが復活しないことを確認済み。

**2026-07-27**: 専用 VM `winremap-test` へ載せ替えた。`-DumpUia` の 5 チェックが 5/5 pass、`02-config-display` が PASS（エージェント実行 1 分 55 秒、リセットは電源オフから 58 秒）。

**2026-07-28（Phase A 完了確認）**: **全シナリオを 2 回通し、6 件すべてが両回 PASS で一致**した（`00-uia-actuation` を含む。通し 1 回あたり約 20 分）。エージェント部分の所要時間は下表のとおりで、**同じ判定でも 1〜2 分ぶれる**（プロンプトは同一で、ぶれているのは実行時間であって結論ではない）。

| シナリオ | 1 回目 | 2 回目 | 判定 |
|---|---|---|---|
| `00-uia-actuation`（AI 無し） | 5/5 | 5/5 | PASS |
| `01-settings-window` | 1:39 | 1:06 | PASS |
| `02-config-display` | 2:29 | 2:21 | PASS |
| `03-tray-actions` | 3:18 | 3:26 | PASS |
| `04-log-window` | 1:22 | 1:59 | PASS |
| `05-remap-notepad` | 3:34 | 1:19 | PASS |

2 回目は 05 の実行中にバックグラウンドタスクが外から停止させられたため、05 のみ revert からやり直して取り直した（01〜04 は停止前に PASS 済み）。**シナリオごとに revert から始まるので、取り直しは前の状態を引き継がない。**

**2026-07-28（Phase B のハーネス検査を追加後）**: `00-cli-smoke`（8 チェック）と `00-regression`（14 チェック）を加えた**全 8 件が PASS**（通し約 40 分）。仕分けと手動最小集合は [v0.5 受け入れチェックリスト](v0.5/03_acceptance-checklist.md) を参照。

トレイを使うシナリオは、実行前にアイコンの昇格が成功したこと（`tray icon: promoted=1`）をランナーが確認する。ここで失敗した場合はシナリオを実行せず `SETUP FAILED` とする — 昇格の空振りは、エージェントが存在しないアイコンを探し続けた末に**アプリの不具合そっくりの失敗**として現れるため。

`04` で分かったこと: **ログウィンドウには「どの設定ファイルを読み込んだか」が出ない。** 起動時の `N keymap(s) loaded from <path>` は `notify::console_line`（`src/main.rs`）にしか流れず、ログウィンドウへは渡っていない。一方でトレイからの再読み込みは `reload_ok` をログウィンドウに出す（`src/tray.rs`）。この非対称をどうするかはオーナー判断待ちで、シナリオは現在の実挙動（起動バナーが読めること）を検証する。

`_` で始まるファイル（`_90-probe-key-injection`・`_91-explore-tray`）は**ハーネス自身の診断**で、`all` には含まれない。名前を指定して単体で回す。「リマップが効かない」ように見えたとき、原因が WinRemap かテスト側かを先に切り分けるために使う。

シナリオ 05 だけは `examples\minimal.toml` ではなく `tests/ui/fixtures/uitest.toml`（`C-h` → `x`）を使う。理由はそのファイルの先頭に書いてあるが、要点は**判定を「文字が消える」ではなく「文字が入る」に置く**ためである（下記の作法）。

起動は必ず `--config <パス> --lang en` を付ける。**`--lang en` はゲストのロケール（日本語）に関係なく UI 文字列を英語に固定する**ためで、これが無いとプロンプト中の要素名が環境依存になる。

### トレイの扱い

- **通知領域のオーバーフロー（∧）は UI 自動化では開けない。** エージェントは ∧ にホバーしたまま 14 分間開けず、探索フェーズごと失敗した。そこで**テスト前にアイコンをタスクバー上へ出す**（`tests/ui/guest/promote-tray-icon.ps1` が `HKCU\Control Panel\NotifyIconSettings` の `IsPromoted` を立てる）。ランナーがトレイを使うシナリオの前に自動実行する
- **トレイの右クリックメニューは UIA に出たり出なかったりする。** Win32 のポップアップ（クラス `#32768`）で、**中身のない `Pane`** にしか見えないことがある（項目が 1 つもツリーに出ない）。.NET の `System.Windows.Automation` からは常にそう見え、terminator MCP でも 2026-07-26 の通し実行で 4 本中 1 本が「項目ゼロ」に当たって落ちた。**AccessKit とは無関係で、Windows のメニューの性質である**
  - 対処は 2 つ。**① メニューは操作にだけ使い、判定はメニュー以外で行う。** シナリオ 03 は有効/無効の判定を**トレイアイコンの名前**（ツールチップ。`WinRemap — 1 keymap(s)` ⇔ `WinRemap (disabled)`）で行う。アイコン自体は常にツリーに出る
  - **② 見つからないときは Windows 本来の操作に落とす。** メニューを開いた状態で頭文字キーを押す。一致が 1 つなら**その場で実行される**（`E`=Enabled、`R`=Reload config、`Q`=Quit）。複数一致なら選択されるだけなので Enter を続ける（`S` は Settings と Show log の 2 つに一致するため、1 回＋Enter で Settings、2 回＋Enter で Show log）
  - シナリオにこれを書いていないと、エージェントは**アプリの不具合として報告して終わる**（実際にそう報告された）

### 設定・ログウィンドウの中身（v0.5.0 で解決済み）

**v0.4 まではスクリーンショットを撮って AI に読ませていた。v0.5.0 からは UIA セレクタで読めて押せる。**

- 見えなかった理由は「OpenGL で描いているから」**ではない**（当初そう書いていたが誤り。オーナー指摘 2026-07-26）。UIA への露出は描画方式と無関係で、egui は [AccessKit](https://accesskit.dev/) 経由でアクセシビリティツリーを出せる。原因は次の 2 点だった
  1. `Cargo.toml` が eframe を `default-features = false` で取っており、**既定機能である `accesskit` が外れていた**（`accesskit_winit` → `accesskit_windows` の UIA プロバイダーが binary に入らない）
  2. 有効にしても足りない。**eframe 0.35 は AccessKit アダプターを ROOT ビューポートにしか作らない**（`ViewportId::ROOT` 決め打ち）。WinRemap の root は不可視 1×1 のホストで、設定・ログは遅延子ビューポート（[ADR 0037](v0.2/decisions/0037-gui-invisible-host-viewport.md)）なので、実ウィンドウにはアダプターが付かなかった
- 対処は [ADR 0055](v0.5/decisions/0055-accesskit-for-child-viewports.md)。eframe をフォークして**ウィンドウを持つ全ビューポートにアダプターを付け**、`[patch.crates-io]` で暫定的に指している（上流にマージされたら外す）
- 実測結果は[調査記録](v0.5/notes/20260726_accesskit-child-viewport.md)。設定ウィンドウ 43 要素（キーマップ選択で 59、編集モードで 72）、ログウィンドウ 7 要素。`Button 'Edit'` は UIA から押せる

#### セレクタは推測せず採取する

```powershell
.\run-vm-ui-test.ps1 -DumpUia
```

シナリオを走らせず、設定ウィンドウとログウィンドウの UIA ツリー（role・name・対応パターン・値）をそのまま出す（`tests/ui/guest/dump-uia.ps1`）。**GUI を変えたら流し直し、シナリオのセレクタを合わせる。**

ここに AI を入れてはいけない。要素が見つからないエージェントは別のものを探しに行って**その結果を報告する**ため、「ウィンドウが何も出していない」と「エージェントがウィンドウを開いていない」が区別できなくなる。実際、探索をエージェントに任せた回は、起動すべき exe を起動しないまま C:\ 全体を検索して「インストールされていない」と報告して終わった

### 期待結果の置き方（実際に踏んだ落とし穴）

- **「入る」で判定し、「消える」で判定しない。** セレクタ経由でキーを送ると**キャレットが文書の先頭へ移動**するため、先頭での Backspace は何も消さない。`C-h` → `Back` を期待結果にすると、リマップが成功していても文字数が変わらず落ちる。挿入（`C-h` → `x`）なら観測が一意になる
- **最新の Notepad の検索/置換は別ウィンドウではなくウィンドウ内パネル**である。ウィンドウ名の一覧だけで「ダイアログは出ていない」と判定すると誤る。Notepad の UI ツリーの中を見るよう明示する
- **ハーネスの挙動とアプリの挙動を混ぜない。** 上の 2 点はどちらも「WinRemap のバグに見えるテスト側の性質」だった。疑わしいときは `_90-probe-key-injection` で先に切り分ける
- **WinRemap 自身のウィンドウが前面にあるときに注入したキーは、WinRemap のフックに届かない**（2026-07-28 測定、`00-log-view` の作成中に判明）。**キーは必ず他のアプリ（Notepad）に対して送ること。**
  - 測定内容: ログウィンドウを前面にして `keybd_event`・`SendInput`（vk）・`SendInput`（スキャンコード）の 3 通りを送ると**ログに 1 行も出ない**。同じスクリプトの同じ関数で、Notepad を前面にして送ると**全部出る**。ウィンドウを 1 枚も開かず `--debug` の出力をファイルに落とした場合も**全部出る**（この 3 例が `00-log-view` の実行記録に残っている）
  - **アプリの欠陥か、注入の性質かは未確定。** ログウィンドウが開いているだけなら問題は起きず、**前面にあるとき**だけ起きる。人が実キーボードで打つ経路とは別なので v0.5 の受け入れは止めないが、**H-2（止まらない・遅れない）を通すときにログウィンドウを前面にした状態も見ること**
  - この切り分けをせずに「新機能が動いていない」と読むと、実際には動いているものを直しにいくことになる（この日、実際にそうなりかけた）
- **ログウィンドウを開いている間、切り替えた先のアプリが前面アプリ行に出ない**（2026-07-29、`00-log-view` に前面アプリ行の検査を足したときに 2 回の実行で観測）。報告されたのは `winremap.exe` と `explorer.exe` だけで、**Notepad は一度も出なかった** — `GetForegroundWindow()` で前面であることを確認し、そのキーがフックに届いている（判定行が出ている）にもかかわらず、である。`Start-Process` 直後のイベントは `winremap.exe` と報告され、その後こちらから明示的に切り替えても新しい行は出なかった。
  - **テスト側の対処**: 前面アプリ行の検査は**どのアプリが出たか**ではなく**行の中身**（`application` の値とキーマップ一覧が揃っているか）で判定する。実際に出たアプリ名は `F|` 行として実行記録に残す
  - **ウィンドウの有無で切り分け済み（3 回目の実行）**。同じスクリプトの同じ関数で、**WinRemap のウィンドウを 1 枚も開かず `--debug` だけで動かすと、Notepad への切り替えは報告される**:

    ```
    07:19:47.368 [window] application = "notepad.exe" — matching keymaps: global
    07:19:47.368 [window]   C:\Program Files\WindowsApps\Microsoft.WindowsNotepad_...\Notepad.exe
    ```

    つまり**ログウィンドウが開いているときだけ**起きる。2026-07-28 に記録した「WinRemap のウィンドウが前面にあるとキーがフックに届かない」と同じ系統の可能性が高い（どちらも WinRemap 側にウィンドウがある場合）。この切り分け記録は `00-log-view` の実行記録に `W|` 行として毎回残る
  - **アプリ側は未確定**。`05-remap-notepad`（Notepad でだけ `C-h` → `x`）は通っており、**exe キャッシュ自体は追従している**ので、リマップの正しさの問題としては観測されていない。**アプリ別キーマップが「特定の切り替えのときだけ効かない」と報告されたら、まずここを疑う**

### 決定論フェーズと探索フェーズ

共通手順の原則どおり、**確定した手順に LLM を挟まない**。要素の role/name は `-DumpUia` で採取し、**その名前をプロンプトに焼き込んで**探索の余地を減らす。プロンプトを変えたら、変更前後で同じ判定になることを 2 回以上の実行で確かめる。

シナリオ 01・02・04 には「**スクリーンショットを撮るな、画像から読むな**」と明記してある。UIA で読めるようになった後もエージェントは画像に逃げがちで、逃げられると「要素が出ていない」という**本来検出したい退行**を見逃す。要素が無いときは無いと報告させる。

## 5. リマップ検証だけが特別な理由

WinRemap は自己送出ループを防ぐため、注入イベント（`LLKHF_INJECTED`）を素通しする（AGENTS.md 不変条件 1）。Terminator の `SendInput` にもこのフラグが立つため、**通常のビルドではテストのキー送出はリマップされない**。

そこで、既定 OFF の Cargo feature `test-inject` を有効にしたビルドでのみ `--accept-injected` を受け付け、**自分が注入したイベント（`dwExtraInfo` のマーカー付き）以外**の注入を変換対象にする（[ADR 0053](v0.4/decisions/0053-test-inject-mode.md)）。自分の注入は従来どおり無条件で素通しするので、自己送出ループは閉じたままである。

- ランナーは、プロンプトに `--accept-injected` が含まれるシナリオを自動的に `--features test-inject` でビルドする。それ以外は出荷物と同じ形のビルドで実行する
- **配布物にはこのコードが入らない。** `cargo build --release`（feature 無し）ではフラグ自体が存在せず、`--accept-injected` は不明な引数として拒否される（`src/main.rs` の単体テストで担保）
- このモードで起動した実行ファイルは、起動ログとトレイメニューの先頭行に `TEST BUILD` を出す

## 6. ハマりどころ

| 事象 | 対処 |
|---|---|
| **revert したのに前回の実行が生き返る／ゲストログが 0 バイトでタイムアウト** | ゴールデンスナップショットが**実行中（メモリ状態込み）で取得**されていると、revert のたびに当時の claude・MCP・アプリが復活し、ログファイルを掴んだまま新しい実行を沈黙させる。根治は `snapshot-golden-vmware.ps1`（電源オフで取得。常駐する WinRemap を落とすため `-AppProcessNames winremap` 必須）で取り直すこと。§2 参照（調査記録は windows-utility の `docs/projects/20260723_Windowsアプリテスト環境構築/12_ゴールデンスナップショットに実行中状態が残っていた件.md`） |
| **別プロジェクトのテストが知らない VM に飛ぶ** | 共通基盤の `.secrets/test-vm.json` を書き換える運用をやめ、`-ConfigPath` で明示する方式に統一した（§2）。ランナーからは共有ファイルを差し替える `Set-DefaultVmConfig` / `Restore-DefaultVmConfig` を削除済み |
| **revert 直後にシナリオが「要素が無い」で落ちる** | VMware Tools が資格情報に応答してから、デスクトップが対話プログラムを受け付けるまでには数分の開きがある。固定時間の `Start-Sleep` で埋めてはいけない。ランナーは `run-in-vm-vmware.ps1 -Restore` に委譲し、**実行可能になるまでの待ち**を基盤側の 1 箇所に集約している |
| **トレイ操作でエージェントが足踏みする** | 通知領域のオーバーフロー（∧）を開いて目的のアイコンを右クリックする操作は、UI 自動化にとって難所である。シナリオ 05 はトレイに触れない（リマップが起きること自体が `--accept-injected` が効いている証拠）。トレイが必要な 01〜04 は、探索フェーズで要素名（オーバーフローボタンはゲストの表示言語の名前になる）を確定してからプロンプトに焼き込む |
| 設定画面が黒い・出ない | Hyper-V では OpenGL が無い。VMware で実行する（`08_egui設定画面のOpenGL問題.md`） |
| ゲストで `claude` が見つからない | PATH 更新が既存セッションに反映されていない。ゴールデンスナップショットはツール導入後に再起動した状態にする |
| 日本語が文字化けする | ゲストの PowerShell 5.1 は BOM 無し UTF-8 を cp932 として読む。**プロンプトは英語で書く**（入口スクリプトは BOM 付きで転送する） |
| UI 自動化が無反応 | session 1（対話デスクトップ）でしか動かない。入口スクリプト経由で実行する |
| ビルドが `アクセスが拒否されました` で失敗 | ホストで `target\release\winremap.exe` が起動中。終了させる（`-SkipBuild` で回避も可） |
| 判定が毎回ぶれる | プロンプトに探索の余地が残っている。§4 のとおり要素名を焼き込む |
| **`reverting to snapshot` から進まない（ゲストは正常に起動している）** | `vmrun start` は VMware Workstation の GUI を起こし、その GUI は vmrun より長く生き残る。**vmrun 自身の終了より多くを待つ書き方**をすると、実質「利用者が VMware を閉じるまで」待つ。`& vmrun ... \| Out-Null` は stdout パイプの EOF を待ち（書き込み端を GUI が継承する）、`Start-Process -Wait` は**プロセスとその子孫全部**を待つ。ランナーはどちらも避けて `Invoke-VmrunHost` で 1 プロセスだけを待つ。**2 時間止まっても 5 分デッドラインは効かない**（デッドラインはその次のポーリングループにあり、そこまで到達しない） |
| **エージェントが「アプリがインストールされていない」と報告して終わる** | 起動すべきは `C:\Test\winremap.exe` であって、インストール済みのアプリではない。エージェントが最初の 1 手に失敗すると探索に流れ、無関係な結論を出す。まず `-DumpUia` で**アプリ側の問題かエージェント側の問題か**を切り分ける（AI を挟まないので混同しない） |

## 7. やらないこと

- **CI（GitHub Actions）への組み込み**: VMware とゲストの認証情報がホスト固有であり、手元実行に留める（v0.4 時点）
- **キー入力内容のログ収集**: テストのためであってもキーロガー化はしない（不変条件 6）
- **`--accept-injected` の配布物への露出**: 利用者向けドキュメント（README・ヘルプサイト）には載せない
