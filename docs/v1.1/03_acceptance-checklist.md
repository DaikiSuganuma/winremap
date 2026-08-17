# v1.1 受け入れチェックリスト

> 元資料: [v1.0 受け入れチェックリスト](../v1.0/03_acceptance-checklist.md)（C-1〜C-5・M-1〜M-7・F-1〜F-5・H-1〜H-10・P-1〜P-10 の直前の版。**MSIX 固有項目の詳しい前準備は [v0.6 §3.1](../v0.6/03_acceptance-checklist.md) が正**）、
> [ADR 0080](decisions/0080-tray-icon-asks-for-the-small-metric.md)（トレイのアイコンは通知領域のサイズを指定して読む）・[ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md)（アイコンは背景に依存しない絵にし、サイズは実 DPI から求める）、[ADR 0025](../v0.1/decisions/0025-display-name-winremap.md)（製品名の表記）、[ADR 0069](../v0.8/decisions/0069-interactive-acceptance-harness.md)・[ADR 0070](../v0.8/decisions/0070-agent-led-acceptance.md)（この文書を読んで、この文書へ追記するハーネス）。
> 調査の元になった資料: [Windows トレイアイコン重複防止と表示仕様](../v1.0/04_Windows%20トレイアイコン重複防止と表示仕様.md)。
> 公式: [`SetThreadDpiAwarenessContext`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext)／[`LoadImage`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-loadimagew)／[Notifications and the Notification Area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area)。

- 作成日: 2026-08-17
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／**実施・記録: オーナー**（自動側の実行は Claude Code が行う）

---

## 0. この文書の目的

v1.1 はまだスコープが決まっていない。この時点で入っているのは、**v1.0.0 のリリース後にオーナーが気づいたトレイまわりの 2 件**である。

1. トレイと設定アプリの一覧で、アイコンが**キーの見えない青い塗りつぶし**になっていた（[ADR 0080](decisions/0080-tray-icon-asks-for-the-small-metric.md)・[ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md)）
2. 同じ一覧とタスク マネージャーで、名前が**小文字の `winremap`** と出ていた（[ADR 0025](../v0.1/decisions/0025-display-name-winremap.md) の適用漏れ）

**この節は、スコープが固まった時点で開発計画（`01_development-plan.md`）に合わせて書き直すこと。**

### ⚠ この区切りで特に注意すること

- **拡大表示（125% 以上）の画面で測ること。** [ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md) 決定 2 は、拡大率 100% では**原理的に効きが見えない** — 100% では小アイコンが 16px で、修正前後どちらも 16px になる。これは [ADR 0076](../v1.0/decisions/0076-read-cursors-unscaled.md) が 3 版にわたって捕まらなかったのと同じ形の落とし穴である。画面の拡大率を確かめてから始めること
- **⚠ I-3 は「新規インストール後」でなければ測れない。** 設定アプリが描いているのは、行のレジストリに保存された `IconSnapshot` という PNG で、**行が作られたときに一度だけ撮られ、アプリが新しいアイコンで再登録しても更新されない**（[ADR 0081](decisions/0081-icon-must-not-depend-on-its-background.md) 実測 2）。既存の行を見ても古い絵のままなので、**通過にならないどころか、直っているものを落とす**。§1.1 の手順で新しい行を作ってから測ること
- **リリースビルドで通す。** 版番号がトレイのメニュー先頭で確かめられる

---

## 1. 前準備

### 1.1 I-3 のための新しい行

**`-Prepare I`。** ハーネスが次を行う。実機のレジストリを触るので、**何をするかを読み上げてから実行する**。

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
- **既に配布した 1.0.0 の利用者の行は直らない。** 1.1 を入れればパスが変わって新しい行ができるので、そこで直る

---

## 5. 記録欄（散文）

（未実施）

---

## 6. 対話ハーネスの記録

| # | 判定 | 記録 |
|---|---|---|
| | | （未実施） |
