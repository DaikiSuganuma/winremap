# v1.0.1 受け入れチェックリスト

> 元資料: [v1.0 受け入れチェックリスト](../v1.0/03_acceptance-checklist.md)（C-1〜C-5・M-1〜M-7・F-1〜F-5・H-1〜H-10・P-1〜P-10 の直前の版。**MSIX 固有項目の詳しい前準備は [v0.6 §3.1](../v0.6/03_acceptance-checklist.md) が正**）、
> [ADR 0080](decisions/0080-tray-icon-asks-for-the-small-metric.md)（トレイのアイコンは通知領域のサイズを指定して読む）・[ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md)（アイコンは背景に依存しない絵にし、サイズは実 DPI から求める）、[ADR 0025](../v0.1/decisions/0025-display-name-winremap.md)（製品名の表記）、[ADR 0069](../v0.8/decisions/0069-interactive-acceptance-harness.md)・[ADR 0070](../v0.8/decisions/0070-agent-led-acceptance.md)（この文書を読んで、この文書へ追記するハーネス）。
> 調査の元になった資料: [Windows トレイアイコン重複防止と表示仕様](../v1.0/04_Windows%20トレイアイコン重複防止と表示仕様.md)。
> 公式: [`SetThreadDpiAwarenessContext`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext)／[`LoadImage`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-loadimagew)／[Notifications and the Notification Area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area)。

- 作成日: 2026-08-17
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／**実施・記録: オーナー**（自動側の実行は Claude Code が行う）

---

## 0. この文書の目的

**v1.0.1 は修正だけの版である。** 利用者から見える新機能は無い。入っているのは **v1.0.0 のリリース後にオーナーが気づいたトレイまわりの 2 件**で、SemVer どおりパッチとして出す（オーナー決定 2026-08-17）。**この版に開発計画（`01_development-plan.md`）は無い** — 計画を書くほどのスコープが無いことが、パッチである理由そのものだからである。

1. トレイと設定アプリの一覧で、アイコンが**キーの見えない青い塗りつぶし**になっていた（[ADR 0080](decisions/0080-tray-icon-asks-for-the-small-metric.md)・[ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md)）
2. 同じ一覧とタスク マネージャーで、名前が**小文字の `winremap`** と出ていた（[ADR 0025](../v0.1/decisions/0025-display-name-winremap.md) の適用漏れ）

### ⚠ この区切りで特に注意すること

- **拡大表示（125% 以上）の画面で測ること。** [ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md) 決定 2 は、拡大率 100% では**原理的に効きが見えない** — 100% では小アイコンが 16px で、修正前後どちらも 16px になる。これは [ADR 0076](../v1.0/decisions/0076-read-cursors-unscaled.md) が 3 版にわたって捕まらなかったのと同じ形の落とし穴である。画面の拡大率を確かめてから始めること
- **⚠ I-3 は「新規インストール後」でなければ測れない。** 設定アプリが描いているのは、行のレジストリに保存された `IconSnapshot` という PNG で、**行が作られたときに一度だけ撮られ、アプリが新しいアイコンで再登録しても更新されない**（[ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md) 実測 2）。既存の行を見ても古い絵のままなので、**通過にならないどころか、直っているものを落とす**。§1.1 の手順で新しい行を作ってから測ること
- **リリースビルドで通す。** 版番号がトレイのメニュー先頭で確かめられる

---

## 1. 前準備

### 1.1 I-3 のための新しい行

**手作業で行う。** 対話ハーネスは I の区切りの準備（`-Prepare I`）を持っていない。実機のレジストリを触るので、**何をするかを読み上げてから実行する**。

1. `HKCU\Control Panel\NotifyIconSettings` を `.reg` へ退避する
2. 測る exe のパスに対応する行を消す（`ExecutablePath` が一致するサブキー）
3. **Explorer を再起動する** — Explorer は行の状態をメモリに持つので、レジストリを消すだけでは足りない
4. WinRemap を起動する。新しい行が `IconSnapshot` ごと作られる
5. その行の `IsPromoted` を 1 にする（タスクバーに出す）

> **Explorer の再起動で、開いているエクスプローラーのウィンドウは閉じる。** 数秒で戻る。
>
> **WinRemap は `TaskbarCreated` を扱っていない。** Explorer が再起動するとトレイアイコンは戻らないので、手順 4 の起動は再起動の**後**でなければならない。（この挙動自体を直すかどうかは未決。[§4](#4-持ち越しと未決) に置く）

### 1.2 実際に測る値

自動側で先に出しておくと、人が見る前に食い違いが分かる。

| 見るもの | 期待 |
|---|---|
| 新しい行の `IconSnapshot` の寸法 | **拡大率に応じた小アイコンの寸法**（150% なら 24×24。100% なら 16×16） |
| 同じ PNG の中の白っぽい画素の数 | **0 より多い**（0 なら旧素材のまま） |
| exe の `FileDescription` / `ProductName` | `WinRemap` |
| exe の `OriginalFilename` | `winremap.exe`（小文字。[ADR 0025](../v0.1/decisions/0025-display-name-winremap.md) の識別子側） |
| exe の `CompanyName` | `SUGANUMA Daiki` |

---

## 2. 項目（I-1〜I-4）

| # | 何を見るか | 手順 | 通過条件 |
|---|---|---|---|
| **I-1** | **トレイのアイコンが読める** | タスクバーの通知領域の WinRemap のアイコンを見る（隠れていれば `∧` を開く） | ①**キーの列が見分けられる**（青一色の塊になっていない） ②輪郭がぼやけていない |
| **I-2** | **無効のときも読める** | トレイのメニューで「有効」のチェックを外し、アイコンを見る。戻す | ①灰色の面でも**キーの列が見分けられる** ②チェックを戻すと青い面に戻る |
| **I-3** | **設定アプリの一覧で読める** | **§1.1 の手順で新しい行を作ってから**、設定 > 個人用設定 > タスクバー > **その他のシステム トレイ アイコン**を開く | ①行の名前が **`WinRemap`**（大文字の R） ②**アイコンにキーが見える**（青い正方形になっていない） |
| **I-4** | **名前と発行元が正しく出る** | タスク マネージャーを開いて「アプリ」/「バックグラウンド プロセス」で WinRemap を探す。exe のプロパティ > 詳細も見る | ①タスク マネージャーの表示が **`WinRemap`** ②プロパティの説明が `WinRemap`、著作権が `Copyright (c) 2026 SUGANUMA Daiki` |

> **I-3 ② が落ちたときは、まず `IconSnapshot` を疑うこと。** §1.2 の「白っぽい画素の数」が 0 なら、行が作り直されていない（＝手順の失敗）であって、アイコンの不具合ではない。
>
> **I-1 と I-2 は同じ操作の往復で測れる。**

---

## 3. 継承する項目

**v1.0 の C-1〜C-5・M-1〜M-7・F-1〜F-5・H-1〜H-10・P-1〜P-10 をそのまま継承する**（[v1.0 のチェックリスト](../v1.0/03_acceptance-checklist.md)）。この区切りの変更は素材・アイコンの読み込み・バージョンリソースに閉じており、キー変換・設定・GUI には触れていない。

ただし **P 区切り（MSIX）は回し直すこと。** パッケージ資産 30 枚を焼き直しているため、タイル・スタートメニュー・Alt+Tab の絵が変わる。

---

## 4. 持ち越しと未決

- **`TaskbarCreated` を扱っていない。** Explorer がクラッシュ・再起動するとトレイアイコンが戻らず、常駐しているのに終了する手段が無くなる。2026-08-17 の調査中に実際に起きた。直すかどうかは未決 — 直すなら `tray-icon` クレートの側の話になる可能性がある
- **プロセスが起動時に DPI 非認識である。** [ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md) はトレイに閉じて回避したが、`gui/win32.rs` のウィンドウアイコン（[ADR 0038](../v0.2/decisions/0038-gui-win32-module.md)）も同じ `GetSystemMetrics` を使っている。設定ウィンドウ・ログウィンドウのアイコンに同じずれがある可能性がある
- **P-9（Store 版の更新）はこの版でも測っていない。** 次の版で、Store 版 1.0.1 からの更新として測る。そのとき `last-config.txt` が更新をまたいで残るかを見る
- **既に配布した 1.0.0 の利用者の行は直らない。** 1.1 を入れればパスが変わって新しい行ができるので、そこで直る

---

## 5. 記録欄（散文）

### 2026-08-17（自動側）— 全部緑

- `cargo fmt --check`・`clippy`（既定と `test-inject` の 2 本）・`cargo test`（**160 件**）・`site-src\build.ps1 -Check`（23 ファイル）すべて通過
- **VM の UI テスト: 10 スイート 121 チェック全通過、fail 0。** `00-uia-actuation` 5・`00-cli-smoke` 8・`00-regression` 14・`00-log-view` 26・`01-settings-window` 12・`02-config-display` 9・`03-tray-actions` 14・`04-log-window` 11・`05-remap-notepad` 10・`06-foreground-line` 12
- **`probe-ime-cursor.ps1`: 10 項目全通過**
- リリースビルド 2 本とも通過。素の配布ビルドの `FileDescription` / `ProductName` が `WinRemap`、`CompanyName` が `SUGANUMA Daiki`、`FileVersion` が `1.0.1`（**I-4 の自動側の裏付け**）

**テスト件数は v1.0 の 160 件から増減なし。** この版で触ったのは素材・アイコンの読み込み・バージョンリソースだけで、テストのある層（`keymap` / `config`）に手が入っていないため、想定どおりである。

**`03-tray-actions` が通ったことは、アイコン読み込み経路を差し替えてもトレイの構築が壊れていないことを示す。** ただし `build_icon` はフォールバック（`from_resource`）を持つので、**これが通ったことは絵が正しいことの証明にはならない** — I-1〜I-3 は人の目で見ること。

**自動側では I-1〜I-3 は測れない。** アイコンの見え方と設定アプリの一覧は、画素を測る検査を持っていない。

**⚠ `probe-ime-cursor.ps1` の 1 行目が `cursors are read at 32px, in a DPI-unaware context` と言っている。** [ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md) が突き止めた「プロセスが起動時に DPI 非認識である」という事実は、[ADR 0076](../v1.0/decisions/0076-read-cursors-unscaled.md) がカーソル側で既に踏んでいた同じ性質のものだった。**この 2 つが同じ根に繋がっていることは、次にどこかで DPI が絡む不具合が出たときの最初の手がかりになる。**

### 2026-09-14（手動側）— I-1〜I-4 全部通過

- **経緯。** 2026-08-17 に I-1・I-2 を旧図形で見て通過したが、I-3 の途中でアイコンを描き直すことになり中断した。7589d2a（オーナーが Illustrator で描き直した図形）で画素が変わったので I-1〜I-3 を見直した。I-4 はこの日が初回
- **I-3 の行は §1.1 の手順を手作業で行った。** レジストリを退避し、`target\release` の行だけを消し（MSIX 1.0.0.0 の行は残した）、Explorer を再起動してから起動し、`IsPromoted=1` にした
- **`IconSnapshot` は 24×24、白っぽい画素は 48。** 新図形を 24 px で焼いたときの数と一致する（上段 3+2+2+3 列 × 2 行、下段 3+8+3 列 × 2 行）。旧図形では 52 だったので、新しい写しが撮られた裏付けになる
- **奇数座標なので、24 px ではキーの辺に半端な画素（1 画素ぶんの水色）が出る。** オーナーがこのマシンのトレイ（24 px）で見て、I-1 ②「輪郭がぼやけていない」を通過とした。偶数座標への描き直しはこの版ではしない（[06_icon-assets.md](../06_icon-assets.md) §5.3）
- **小マスター（`-small.svg`）は作らず、通常マスターを描き直した。** [ADR 0082](decisions/0082-small-master-for-small-sizes.md) のフォールバックどおり、全サイズを通常マスターから焼いている

### 2026-09-14（P 区切り）— P-10 通過、P-9 は測れない

- **始めた時点で、この機械には Store 版 1.0.0（`SignatureKind: Store`）が入っていた。** 開発者登録で P を回すにはこれを消す必要があり、消すと P-9 の更新元が無くなる。認定後に Store の更新で P-9 → P-10 を測る案と、今消して回す案を示し、オーナーが「Store 版を消して今回す」を選んだ
- **Store 版のパッケージ専用フォルダーに設定があった。** `%LOCALAPPDATA%\Packages\SUGANUMADaiki.WinRemap_pktmgf1zdhxe0\LocalCache\Roaming\winremap` に `personal-ja.toml`（6320 バイト、2026-08-20 更新）と `last-config.txt`（中身は `C:\Users\suganuma\AppData\Roaming\winremap\personal-ja.toml`）。`%APPDATA%\winremap\personal-ja.toml` は 6317 バイト、2026-08-18 更新で、内容が異なる。アンインストールでこのフォルダーは消えるので、先に `C:\Users\suganuma\winremap-store-1.0.0-backup-20260914` と scratchpad へ複製し、ハッシュの一致を確かめた
- `Remove-AppxPackage` で 1.0.0 を削除し、パッケージ専用フォルダーが消えたことを確かめた。`build.ps1 -Register` で登録し、`Version 1.0.1.0`・`SignatureKind None`・`resources.pri: 3664 bytes, altform-unplated x 6` だった。登録後の新しいパッケージ専用フォルダーへ、退避した 2 ファイルを戻した（ハッシュ一致）
- `shell:AppsFolder` から起動した。トレイの行（`ExecutablePath` が `packaging\msix\layout\winremap.exe`）が新しく作られ、`IconSnapshot` は 24×24、白っぽい画素 48。`IsPromoted=1` にした
- P-10 はオーナーが 3 か所を見て通過
- P-1〜P-7 は回していない。`git diff v1.0.0..HEAD` で `AppxManifest.xml`・`build.ps1`・`src/package.rs` に差分が無い
- 登録したパッケージのインストール先は `packaging\msix\layout` である。`build.ps1` はこのフォルダーを毎回作り直す

---

## 6. 対話ハーネスの記録

今回はハーネスを使わず、手作業で見て記録した。

| # | 判定 | 記録 |
|---|---|---|
| I-1 | 通過 | キーの列が見分けられ、輪郭はぼやけていない（新図形・24 px） |
| I-2 | 通過 | 灰色の面でもキーの列が見分けられる。チェックを戻すと青に戻る |
| I-3 | 通過 | 作り直した行で、名前が `WinRemap`、アイコンにキーが見える |
| I-4 | 通過 | タスク マネージャーの表示が `WinRemap`。プロパティの説明が `WinRemap`、著作権が `Copyright (c) 2026 SUGANUMA Daiki` |
| P-1〜P-7 | 未実施 | `git diff v1.0.0..HEAD` で `AppxManifest.xml`・`build.ps1`・`src/package.rs` に差分が無いため回していない |
| P-8 | 測れない | Store 経由のインストールは認定通過後にしか測れない |
| P-9 | 測れない | 更新元の Store 版 1.0.0 を、P を今回すために削除した（オーナーの選択）。次の版で 1.0.1 からの更新として測る |
| P-10 | 通過 | 開発者登録した 1.0.1 で、スタートアップ一覧・タスクバー・スタート メニューの 3 か所とも問題なし、との報告 |
