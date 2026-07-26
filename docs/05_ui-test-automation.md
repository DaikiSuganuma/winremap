# UI テスト自動化（VM ＋ AI エージェント）

> 元資料: windows-utility リポジトリの `.ai-rules/rules/workflows/windows-ui-testing.md`（Windows デスクトップアプリの UI テスト自動化・共通手順）と
> `docs/projects/20260723_Windowsアプリテスト環境構築/11_UIテスト自動化ガイド(WinRemapエージェント向け).md`（WinRemap 固有の差分）。
> 環境の構築・保守はそちらが正であり、本書は**このリポジトリ側の使い方**を記述する。

- 作成日: 2026-07-25
- 作成: Claude Code（AI モデル: claude-opus-5）／レビュー・承認: オーナー
- 関連: [ADR 0053](v0.4/decisions/0053-test-inject-mode.md)（注入イベント受理モード）、[v0.4 開発計画 §6](v0.4/01_development-plan.md)（Phase E）

---

## 1. これは何か

VMware のゲスト Windows（自動ログオン済みのデスクトップ）で Claude Code ＋ Terminator MCP（Model Context Protocol: AI が外部ツールを呼ぶための規格）に画面を操作させ、WinRemap の GUI とリマップ動作を自動で検証する。UI 自動化は画面のある対話セッション（session 1）でしか動かず、常駐アプリとグローバルフックを毎回きれいに消す必要があるため、VM とスナップショットを使う。

実行の入口はこのリポジトリの `tests/ui/run-vm-ui-test.ps1`（ホスト側で実行）。ゲストへコマンドを流す部分は windows-utility の `test-vm/scripts/run-in-vm-vmware.ps1` に委譲する。

```
run-vm-ui-test.ps1（本リポジトリ）
  ├─ vmrun revertToSnapshot ready → start   … 前回の残骸を消す
  ├─ cargo build --release [--features test-inject]
  ├─ vmrun copyFileFromHostToGuest          … winremap.exe と minimal.toml を C:\Test へ
  ├─ run-in-vm-vmware.ps1 -Command "claude -p <シナリオ>"（windows-utility）
  │    └─ ゲスト session 1 で Terminator MCP が UI を操作
  └─ 最終行の PASS / FAIL を集計 → revert
```

## 2. 前提

| 項目 | 値 |
|---|---|
| ホスト | VMware Workstation（Hyper-V の合成ディスプレイは OpenGL 非対応で設定画面が出ない） |
| ゲスト VM | `C:\VMware\win11-test\win11-test.vmx`（接続情報は windows-utility の `.secrets/test-vm.json`） |
| ゴールデンスナップショット | `ready`（ツール導入・認証・Terminator 込み） |
| ゲスト内 | Claude Code（`CLAUDE_CODE_OAUTH_TOKEN` を User 環境変数に）、Terminator MCP `0.24.28` |

**認証情報・VM の接続情報は本リポジトリに置かない。** スクリプトは windows-utility の `.secrets/test-vm.json` を読むだけで、パスワードを出力しない。

## 3. 実行方法

```powershell
cd D:\Projects\GitHub\winremap\tests\ui
.\run-vm-ui-test.ps1                              # 全シナリオ
.\run-vm-ui-test.ps1 -Scenario 05-remap-notepad   # 1 本だけ
```

| オプション | 既定 | 用途 |
|---|---|---|
| `-Scenario` | `all` | `scenarios\` のファイル名（拡張子なし） |
| `-EntryScript` | windows-utility の既定パス | 別マシンでリポジトリの場所が違う場合に指定 |
| `-Snapshot` | `ready` | 戻す先のスナップショット |
| `-TimeoutMin` | `15` | 1 シナリオあたりの上限 |
| `-NoRevert` | — | revert せず今のゲストで実行（**プロンプト調整中のみ**。再現性は失われる） |
| `-SkipBuild` | — | ビルド済みバイナリを再利用 |

終了コードは全シナリオ PASS で 0、1 つでも FAIL または判定不能なら 1。

## 4. シナリオ

`tests/ui/scenarios/*.txt` が 1 ファイル 1 シナリオで、中身はそのまま `claude -p` に渡す英語プロンプトである。**最終行に `PASS` か `FAIL` だけを出す**よう指示してあり、ランナーはそれを拾う。

| ファイル | 内容 | 対応する受け入れ項目 |
|---|---|---|
| `01-settings-window` | 設定画面が開き、要素が UIA に出て `Edit` が押せる | v0.4 A-1 の一部、v0.2 B0-2 |
| `02-config-display` | アドレスバーのパスとキーマップ表示が `minimal.toml` と一致する | v0.2 B1-1・B1-2 の最小構成 |
| `03-tray-actions` | 有効/無効トグル・Reload・Quit | v0.2 A-21 とトレイ操作 |
| `04-log-window` | ログ行・追従チェックボックス・`Clear`／`Copy all` | v0.2 A-10・B1-29 |
| `05-remap-notepad` | Notepad で `C-h` → `x`（文字が入り、置換パネルが開かない）。**トレイには触れない** | v0.1 M2-1 と同じ経路 |

どこまでを自動で済ませ、何が手動に残るかは [v0.4 受け入れチェックリスト](v0.4/03_acceptance-checklist.md) の §G が正である。

**現状（2026-07-25）**: **`.\run-vm-ui-test.ps1` の 1 コマンドで全 5 シナリオが PASS**（エージェント実行部分は 1 本あたり 2〜7 分。シナリオごとに revert・起動・配置が入るため通しでは 25 分前後）。ゴールデンスナップショットは同日に windows-utility 側で電源オフ状態へ取り直され、revert 直後に残骸プロセスが復活しないことを確認済み。

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
| **revert したのに前回の実行が生き返る／ゲストログが 0 バイトでタイムアウト** | ゴールデンスナップショットが**実行中（メモリ状態込み）で取得**されていると、revert のたびに当時の claude・MCP・アプリが復活し、ログファイルを掴んだまま新しい実行を沈黙させる。ランナーは起動直後に `Clear-StaleRun` で残骸を落として回避する。**根治は windows-utility 側でスナップショットを電源オフで取り直すこと**（調査記録は同リポジトリの `docs/projects/20260723_Windowsアプリテスト環境構築/12_ゴールデンスナップショットに実行中状態が残っていた件.md`、恒久ルールは `.ai-rules/rules/workflows/windows-ui-testing.md`） |
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
