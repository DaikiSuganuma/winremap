# **Windows「その他のシステムトレイアイコン」における登録重複防止と表示制御の技術仕様分析**

- 作成日: 2026-08-17
- 作成: Gemini Deep Research（オーナーが実施し、本リポジトリへ配置）
- 追記・実機検証: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- 関連: [ADR 0080](../v1.0.1/decisions/0080-tray-icon-asks-for-the-small-metric.md)（トレイのアイコンは通知領域のサイズを指定して読む）、[ADR 0081](../v1.0.1/decisions/0081-icon-must-not-depend-on-its-background.md)（アイコンは背景に依存しない絵にし、サイズは実 DPI から求める）、[ADR 0025](../v0.1/decisions/0025-display-name-winremap.md)（製品名の表記）

> **本文中の「実測（2026-08-17）」の枠は、この調査レポートをオーナーの開発機（Windows 11 Pro 26200、表示スケール 150%）で突き合わせた結果として Claude Code が追記したものである。** レポート本体の記述には手を入れていない。

Windows 11の設定アプリケーション（「設定」\>「個人用設定」\>「タスクバー」\>「その他のシステムトレイアイコン」）における通知領域アイコンの管理は、Windowsシェル（Explorer）の内部状態およびレジストリデータベースと直接連携しています。アプリケーションのアップデートごとにアイコンエントリが重複・増殖する現象は、シェルのエンティティ識別メカニズム、更新時のファイル配置、およびプロセスのライフサイクル管理の不一致に起因します。本レポートは、重複が発生するOS内部のメカニズム、設定画面における表示メタデータの決定規則、ならびにこれらを恒久的に制御するための技術仕様と実装方針を体系的にまとめたものです。

## **1\. 通知領域登録アーキテクチャと設定アプリの連動メカニズム**

Windowsにおけるシステムトレイアイコン（通知領域アイコン）は、Win32 APIの Shell\_NotifyIcon 関数および NOTIFYICONDATA 構造体を通じて登録、変更、削除が行われます1。OSは登録された各アイコンの表示状態（タスクバー上に常に表示する「プロモート」状態か、オーバーフローメニュー内に隠す状態か）をユーザープロファイルごとに永続化します3。

### **1.1 OSバージョンによる永続化ストレージの変遷**

通知領域の設定情報を保持する内部構造は、Windows 10以前とWindows 11で大幅に刷新されています。

| 比較項目 | Windows 10 以前 (Legacy) | Windows 11 (Modern) |
| :---- | :---- | :---- |
| **主要レジストリパス** | HKCU\\Software\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\CurrentVersion\\TrayNotify \[cite: 5, 6\] | HKCU\\Control Panel\\NotifyIconSettings \[cite: 4, 5, 7\] |
| **データ構造** | IconStreams および PastIconsStream 内にバイナリストリームとしてシリアライズ（ROT13暗号化を含む）5 | アプリケーションごとに一意なIDを持つサブキーとして構造化保存4 |
| **設定反映の即時性** | レジストリ変更時にExplorerプロセスの再起動が必要5 | IsPromoted などの値変更がExplorerへ即時反映4 |
| **設定画面** | コントロールパネルの「通知領域アイコン」3 | 設定アプリ「個人用設定 \> タスクバー \> その他のシステムトレイアイコン」11 |

### **1.2 Windows 11 NotifyIconSettings のスキーマ定義**

Windows 11においてトレイアイコンが初めて登録されると、シェルは HKCU\\Control Panel\\NotifyIconSettings 配下に一意のハッシュまたはIDを冠したサブキーを自動生成し、以下の属性値を格納します4。

| レジストリ値名 | データ型 | 説明およびシェルの用途 |
| :---- | :---- | :---- |
| **ExecutablePath** | REG\_SZ | 登録元実行ファイルの絶対パス（Known Folder CLSIDが使用される場合あり）8 |
| **InitialTooltip** | REG\_SZ | Shell\_NotifyIcon(NIM\_ADD) 時に渡された初期ツールチップ文字列8 |
| **Publisher** | REG\_SZ | 実行ファイルのデジタル署名またはバージョン情報リソースから抽出された発行元9 |
| **IconGuid** | REG\_SZ | NIF\_GUID 指定時に割り当てられたGUID文字列9 |
| **UID** | REG\_DWORD | ウィンドウハンドルと組み合わせて識別するアプリケーション定義の整数ID8 |
| **IsPromoted** | REG\_DWORD | タスクバー上に常時表示（1）するかオーバーフロー領域に隠す（0）かの設定値4 |

設定アプリは起動時にこのレジストリパス配下を走査し、登録済みの全アイテムをリストアップして表示します4。

> **実測（2026-08-17）— この表に載っていない値が 1 つあり、それが表示の要だった。**
>
> | レジストリ値名 | データ型 | 説明 |
> | :---- | :---- | :---- |
> | **IconSnapshot** | REG\_BINARY | **PNG そのもの。設定アプリの一覧が描いているのはこれ**（`hIcon` を都度読みに行くのではない）。行が作られたときに一度だけ撮られ、**アプリが別のアイコンで再登録しても更新されない** |
>
> 実測した寸法は、そのアプリが `Shell_NotifyIcon` に渡した `hIcon` の寸法そのままだった（WinRemap 修正前 32×32、Aqua Voice 128×128）。設定アプリの行は 36px 前後で描くので、**小さすぎる `hIcon` を渡すとここで拡大される**。§4.2 の追記を参照。
>
> なお **`Publisher` は実機の行には存在しなかった**（WinRemap・Aqua Voice・rundll32 のいずれにも無い）。実際に存在したのは `UID`・`ExecutablePath`・`InitialTooltip`・`IconSnapshot`・`IsPromoted` の 5 つである。

## **2\. アプリケーション更新時にアイコンが増殖・重複する根本原因**

設定画面内に同一アプリケーションのアイコンがバージョン更新のたびに蓄積していく現象は、主に以下の技術的要因によって引き起こされます。

### **2.1 実行ファイルパスの動的変更**

一部の自動更新フレームワーク（Squirrel.Windowsや初期のElectronアップデータなど）は、バージョン更新ごとに AppData\\Local\\\<App\>\\app-1.0.0\\app.exe から AppData\\Local\\\<App\>\\app-1.0.1\\app.exe のように実行パスを変更して配置します6。シェルはアイコンの同一性判定において実行ファイルの絶対パスを主要キーとして参照するため、パスが変化した瞬間に新規アプリケーションとして認識し、NotifyIconSettings 内に新しいサブキーを生成します4。古いバージョンのパスに紐付いたエントリは自動削除されず、過去の履歴として設定画面に残留し続けるため、更新回数に応じてアイコンが増殖します5。

> **実測（2026-08-17）— 「設定画面に残留し続ける」は条件付きである。**
>
> レジストリのサブキーが自動削除されないのは確認できた。ただし**設定画面は、`ExecutablePath` の指す exe が現在もディスク上に存在する行しか表示しない**。存在しないパスの行は「削除されない」が「表示されない」。
>
> | アプリ | レジストリの `ExecutablePath` | exe の実在 | 設定画面 |
> |---|---|---|---|
> | Copilot | `WindowsApps\Microsoft.Copilot_**1.25121.84.0**_x64__…\Copilot.exe` | 無し（現在は `150.0.4078.96`） | 出ない |
> | Copilot | `Program Files (x86)\Microsoft\Copilot\Application\mscopilot.exe` | 有り（固定パス） | 出る |
> | Teams | `WindowsApps\MSTeams_**26183**.1903.4892.4448_x64__…\ms-teams.exe` | 無し（現在は `26198.304.4946.9672`） | 出ない |
> | Aqua Voice | `AppData\Local\aqua-voice\app-**0.18.8**\Aqua Voice.exe` | 有り（Squirrel が前版を残す） | 出る |
> | Aqua Voice | `AppData\Local\aqua-voice\app-**0.18.22**\Aqua Voice.exe` | 有り | 出る |
> | WinRemap | `WindowsApps\SUGANUMADaiki.WinRemap_**0.9.0.0**_x64__…\winremap.exe` | 無し（1.0.0.0 に置換） | 出ない |
>
> つまり §2.1 の記述が当てはまるのは、**Squirrel のように旧バージョンのフォルダーがディスクに残る**方式のときである。Aqua Voice が 2 行見えるのはこれ。一方 **MSIX は更新時に旧バージョンのフォルダーごと消える**ため、その行は自動的に一覧から見えなくなり、更新を重ねても増えない。Copilot と Teams が何度更新されても 1 行しか見えないのはこの理屈による。
>
> **WinRemap にとっての結論**: MSIX で配布している限り、利用者の設定画面でアイコンが増殖することはない。開発機で 5 行に見えていたのは、`target\debug` / `target\release` / `packaging\msix\layout` / 受け入れテスト用の一時フォルダーという**ビルド成果物が同時にディスク上へ存在し続ける**開発機固有の事情による。
>
> なお行の同一性は `ExecutablePath` **と `UID` の組**である。同じ exe でも `UID` が違えば別行になる（実測では `OUTLOOK.EXE` が `UID` 12345 と 0 で 2 行）。

### **2.2 パス文字列の大文字・小文字の不一致**

NTFSファイルシステム自体はパスの大文字と小文字を区別しませんが、シェルのレジストリ登録・突合ロジックは完全一致文字列比較を行う場合があります12。例えば、更新パッチ適用前後で実行パスの表記が ...\\AIPTask.exe から ...\\AipTask.exe に変更された場合、同一のバイナリであってもシェルは別エンティティとみなし、設定画面に重複して登録されます12。

### **2.3 guidItem（NIF\_GUID）のスプーフィング防止機構とパスの不一致**

Windows 7以降で導入されたGUID識別（NIF\_GUID）方式では、初回の Shell\_NotifyIcon(NIM\_ADD) 実行時にGUIDと実行ファイルのフルパスがOS内部で強固にバインドされます2。未署名のアプリケーションが異なるパスから同一のGUIDを用いてアイコンを登録しようとすると、OSのスプーフィング（なりすまし）保護機能が作動し、Shell\_NotifyIcon は ERROR\_TIMEOUT（エラーコード 1460）を返して登録に失敗します2。この登録失敗を回避するために起動時やパス変更時にランダムな新しいGUIDを生成する回避策をとると、更新ごとに新たなGUIDサブキーが生成され、設定画面の重複が恒常化します6。

### **2.4 終了処理におけるクリーンアップ漏れ**

アプリケーション終了時に Shell\_NotifyIcon(NIM\_DELETE) を明示的に呼び出さずにプロセスが強制終了した場合、タスクバー上にゴーストアイコンが残るだけでなく、シェル側の内部状態にも未解放の参照が残り、次回起動時の整合性判定に悪影響を及ぼす可能性があります1。

## **3\. 重複防止のためのアーキテクチャ設計と実装プラクティス**

トレイアイコンの重複を根絶し、バージョン更新後もユーザーの表示設定（ピン留め状態）を完全に維持するためには、シェルの識別要件を満たすデプロイおよび実装設計が必要です。

### **3.1 アイコン識別方式の特性比較**

| 識別方式 | 識別パラメータ | 利点 | 制約事項・留意点 |
| :---- | :---- | :---- | :---- |
| **GUID方式 (NIF\_GUID)** | guidItem (一意な固定GUID)1 | ・同一プロセス内の複数トレイアイコンを確実に分離可能1 ・Modern APIとの整合性が高い1 | ・実行ファイルパスの変更に対して極めて厳格2 ・パス変更時の設定引継ぎには電子署名が必須2 |
| **レガシー方式 (HWND \+ uID)** | hWnd \+ uID (整数値)2 | ・実装がシンプルであり、ポータブル実行時にパス束縛エラーを起こしにくい16 | ・更新時に実行パスが変わると確実に重複エントリが生成される6 |

### **3.2 静的インストールパスの維持（推奨）**

最も堅牢な重複防止策は、アプリケーションの配置ディレクトリを固定することです（例: C:\\Program Files\\YourApp\\YourApp.exe または %LocalAppData%\\Programs\\YourApp\\YourApp.exe）。更新時はインプレースで同一パス上のバイナリを上書きします。パスが完全に不変である限り、NIF\_GUID 方式と HWND \+ uID 方式のいずれを用いても、シェルは同一のアプリケーションとして認識し、設定画面の重複は一切発生しません2。

### **3.3 Authenticode署名による設定マイグレーションの有効化**

更新アーキテクチャの都合上、バージョン番号を含むディレクトリへの配置など実行パスの変更が避けられない場合は、更新前後のバイナリに同一のコードサイニング証明書によるAuthenticode署名を付与する必要があります2。Windowsシェルには、同一の発行元によって署名されたバイナリ間であれば、パスが変更された場合でも既存のGUID登録およびユーザーの設定情報（IsPromoted の状態等）を新しい実行パスへ自動的にマイグレーションする例外規定が組み込まれています2。

### **3.4 固定ランチャー（スタブ）アーキテクチャの導入**

コードサイニング証明書を用意できない環境でバージョン別フォルダ展開を伴うフレームワークを使用する場合は、固定パスに軽量なランチャースタブ（AppLauncher.exe）を配置し、ランチャー側がトレイアイコンを管理して実処理バイナリを子プロセスとして制御する構造を採用します。これにより、シェルに登録される実行パスを恒久的に固定化できます。

### **3.5 インストーラーによるレジストリ清掃ルーチン**

アンインストール時やメジャーアップデート時には、インストーラー（WiX、Inno Setup等）のスクリプトから自社アプリケーションの HKCU\\Control Panel\\NotifyIconSettings サブキーを走査・削除する処理を組み込み、古い残骸エントリを確実にパージします5。

> **実測（2026-08-17）— WinRemap には不要かつ実装不能。** §2.1 の追記のとおり、MSIX では旧バージョンのフォルダーが消えた時点でその行は一覧に出なくなるので、掃除する動機が無い。加えて MSIX は HKCU を書き換える任意のインストールスクリプトを実行できないため、そもそも組み込めない。

## **4\. 設定画面における名称・アイコン画像・ツールチップの表示規則**

設定アプリの「その他のシステムトレイアイコン」画面に描画される項目は、実行ファイルのメタデータおよび NOTIFYICONDATA に渡されたデータから次のように抽出されます。

### **4.1 アプリケーション名の決定アルゴリズム**

設定画面に表示されるタイトル文字列は、実行ファイルのPEヘッダー内に埋め込まれたバージョン情報リソース（VS\_VERSION\_INFO）から優先的に取得されます9。

> 1. **FileDescription（ファイルの説明）**: 最優先で表示名称として採用されます。  
> 2. **ProductName（製品名）または OriginalFilename**: FileDescription が定義されていない場合に参照されます。  
> 3. **ファイル名（フォールバック）**: バージョン情報リソースが存在しない場合、実行ファイルのファイル名（例: app.exe）または初回登録時の szTip（InitialTooltip）が使用されます5。

多言語対応を行う場合は、バージョンリソースの StringFileInfo ブロックを各言語ロケールに合わせて定義することで、OSの表示言語に応じた適切な名称が設定画面に反映されます。また、発行元（Publisher）情報には、バイナリのデジタル署名証明書の組織名、またはリソース内の CompanyName（会社名）が適用されます9。

> **実測（2026-08-17）— この節のとおりだった。対応済み。** v1.0.0 の exe は `FileDescription` / `ProductName` が crate 名そのままの小文字 `winremap`、`OriginalFilename` と `CompanyName` は空だった（`winresource` は前 2 つを crate 名で埋め、後ろ 2 つには既定値を入れない）。結果として設定画面にもタスク マネージャーにも小文字の `winremap` が出ていた。
>
> `build.rs` で明示的に設定して解消した。[ADR 0025](../v0.1/decisions/0025-display-name-winremap.md) に従い、**表示テキストである `FileDescription` / `ProductName` は `WinRemap`、ファイル名を指す `OriginalFilename` は小文字の `winremap.exe`** としている。`CompanyName` は MSIX マニフェストの `PublisherDisplayName` と同じ `SUGANUMA Daiki`。
>
> なお、この一覧の行の名前が**パッケージの `DisplayName` ではなく exe のバージョンリソース**から来ていたことが、アイコンの不具合（§4.2 の追記）の切り分けでも決め手になった — シェルは MSIX 版に対してもパッケージのロゴではなく exe 側を見ている。

### **4.2 アイコン画像のリソース仕様と高DPI対応**

設定画面およびタスクバーのアイコン描画には、NOTIFYICONDATA.hIcon で渡されたアイコンハンドルが使用されます2。  
表示品質を担保するためには、従来の LoadIcon や LoadImage ではなく、Common Controlsライブラリの LoadIconMetric API（サイズフラグに LIM\_SMALL を指定）を用いてロードします1。高DPIディスプレイ（125%、150%、200%等）において、システムが要求する正確なメトリック（16x16、20x20、24x24、32x32ピクセル等）でラスタライズが行われ、ぼやけやジャギーの発生を防止できます1。バイナリには 16x16 から 256x256 までのマルチ解像度を含んだ .ico ファイルを埋め込む必要があります。

> **実測（2026-08-17）— この節が手がかりになったが、本節だけでは足りなかった。詳細は [ADR 0080](../v1.0.1/decisions/0080-tray-icon-asks-for-the-small-metric.md)・[ADR 0081](../v1.0.1/decisions/0081-icon-must-not-depend-on-its-background.md)。**
>
> トレイと設定画面で WinRemap のアイコンが**キーの見えない青い塗りつぶし**になっていた。`assets/kbd.ico` は 16 / 24 / 32 / 48 / 256 px を持ち、exe への埋め込みも正常だったので素材の欠落ではない。原因は独立に 3 つあった。
>
> **原因 1 — 渡していたアイコンの寸法。** `tray-icon` クレートの `Icon::from_resource(ordinal, None)` は内部で `LoadImageW(…, 0, 0, LR_DEFAULTSIZE)` を呼ぶ。`LR_DEFAULTSIZE` が意味するのは**大アイコン**（`SM_CXICON`）であって、通知領域が使う小アイコン（`SM_CXSMICON`）ではない。
>
> **⚠ ここで一度間違えた。** `SM_CXSMICON` を明示すれば直ると考えたが、**寸法はプロセスの DPI 認識で決まる**。winremap のプロセスはトレイを組み立てる時点で `DPI_AWARENESS_UNAWARE` であり（winit が認識を宣言するのはイベントループ生成時、GUI スレッドで起動後）、DPI 関連の API がすべて 96 dpi の値を返す。**別プロセス（DPI 認識のある PowerShell）で測った値を、アプリが見る値だと思い込んだのが誤り**だった。
>
> 開発機（表示スケール 150%）での実測値:
>
> | | winremap の中（DPI 非認識） | DPI 認識のあるプロセス |
> |---|---|---|
> | `SM_CXSMICON` | **16** | 24 |
> | `SM_CXICON` | **32** | 48 |
>
> つまり修正前は 32px の面を渡してシェルが 24px へ縮小しており、`SM_CXSMICON` を素朴に指定すると 16px になって**縮小が拡大に変わるだけ**だった。24px 枠での見え方を描き比べると、拡大が最も眠く、原寸が最良、縮小はほぼ原寸並みである。`SetThreadDpiAwarenessContext` で問い合わせの間だけこのスレッドを per-monitor 認識にし、`GetSystemMetricsForDpi` を引くことで、`.ico` に実在する 24px の面を渡せるようになった。
>
> 本節が推奨する `LoadIconMetric(LIM_SMALL)` を採らなかったのは、寸法の根拠がプロセスの DPI 文脈である点は同じで、非認識のままでは 16 が返るため。`windows` クレートへの `Win32_UI_Controls` フィーチャー追加も要る。
>
> **原因 2 — 設定アプリの一覧は `hIcon` を直接描かず、`IconSnapshot` を描く。** 本節は「設定画面およびタスクバーのアイコン描画には `NOTIFYICONDATA.hIcon` が使用される」とだけ書いているが、設定アプリが実際に読んでいるのは**レジストリの行に保存された PNG**である（§1.2 の追記を参照）。これは**行が作られたときに一度だけ撮られ、アプリが新しいアイコンで再登録しても更新されない**。素材を直しても画面が変わらなかったのはこれが理由で、行を削除し **Explorer を再起動してから**アプリを起動し直すと、新しい絵で撮り直される（Explorer は行の状態をメモリに持つため、レジストリを消すだけでは足りない）。
>
> **原因 3 — 設定アプリは各行のアイコンをアクセント色の台座の上に描く。** 台座は `#007AD5`、WinRemap のアイコン本体は `#0078D4` でほぼ同色だった。しかもキーが `fill-rule="evenodd"` の単一パスで本体をくり抜いた**穴**だったため、穴から台座の同じ青が透けて絵全体が台座に溶けていた。行の青領域を測ると 36 × 36 の正方形で、アイコン素材の絵の比は 1.40 : 1 — **描かれていたのは台座だけだった**。本体とキーを別パスに分け、キーを不透明な白で塗って解決した。
>
> 教訓は、**アイコンは置かれる背景を知らない**ということである。明るいタスクバー・暗いタスクバー・アクセント色の台座のどれでも読めることが要件であり、それはアルファの抜きでは満たせない。面ごとに手当てを足す方式（MSIX の `_altform-unplated` はその 1 周目だった）は、面が増えるたびに同じ不具合を再発する。

### **4.3 ツールチップの制御仕様**

NOTIFYICONDATAW の szTip メンバーにはUTF-16文字列を設定します2。NOTIFYICON\_VERSION\_4（または uVersion \= 5）を適用した場合、標準ツールチップの最大長は終端文字を含めて128文字です1。初回の Shell\_NotifyIcon(NIM\_ADD) 時に渡された szTip はレジストリの InitialTooltip に保存され、設定アプリの内部リスト管理や検索インデックスに使用されます8。

## **5\. Win32 実装リファレンス**

以下は、Windows 11環境において固定GUIDを用い、高DPI対応とModern仕様（NOTIFYICON\_VERSION\_4）を満たしたトレイアイコン管理の実装例です。

C++  
\#**include** \<windows.h\>  
\#**include** \<shellapi.h\>  
\#**include** \<commctrl.h\>  
\#**include** \<strsafe.h\>

\#**pragma** comment(lib, "comctl32.lib")

// アプリケーション固有の固定GUID（guidgen.exe等で生成した固定値）  
static const GUID APP\_TRAY\_ICON\_GUID \=   
{ 0x7c5a40ef, 0xa0fb, 0x4bfc, { 0x87, 0x4a, 0xc0, 0xf2, 0xe0, 0xb9, 0xfa, 0x8e } };

\#**define** WMAPP\_NOTIFYCALLBACK (WM\_APP \+ 1\)  
\#**define** IDI\_TRAY\_APP\_ICON    101

// トレイアイコンの登録（アプリケーション起動時）  
HRESULT RegisterTrayIcon(HWND hWnd, HINSTANCE hInstance)  
{  
    NOTIFYICONDATAW nid \= {};  
    nid.cbSize \= sizeof(NOTIFYICONDATAW);  
    nid.hWnd \= hWnd;  
    nid.uFlags \= NIF\_ICON | NIF\_TIP | NIF\_MESSAGE | NIF\_GUID | NIF\_SHOWTIP;  
    nid.guidItem \= APP\_TRAY\_ICON\_GUID;  
    nid.uCallbackMessage \= WMAPP\_NOTIFYCALLBACK;

    // 高DPI対応の小アイコン取得  
    LoadIconMetric(hInstance, MAKEINTRESOURCE(IDI\_TRAY\_APP\_ICON), LIM\_SMALL, &(nid.hIcon));

    // ツールチップ文字列の設定  
    StringCchCopyW(nid.szTip, ARRAYSIZE(nid.szTip), L"Application Status: Running");

    BOOL result \= Shell\_NotifyIconW(NIM\_ADD, \&nid);  
    if (\!result)  
    {  
        if (nid.hIcon) DestroyIcon(nid.hIcon);  
        return E\_FAIL;  
    }

    // NOTIFYICON\_VERSION\_4 の動作仕様を有効化  
    nid.uVersion \= NOTIFYICON\_VERSION\_4;  
    Shell\_NotifyIconW(NIM\_SETVERSION, \&nid);

    if (nid.hIcon) DestroyIcon(nid.hIcon);  
    return S\_OK;  
}

// ツールチップおよびアイコンの動的更新  
HRESULT UpdateTrayTooltip(const wchar\_t\* tooltipText)  
{  
    NOTIFYICONDATAW nid \= {};  
    nid.cbSize \= sizeof(NOTIFYICONDATAW);  
    nid.uFlags \= NIF\_TIP | NIF\_GUID | NIF\_SHOWTIP;  
    nid.guidItem \= APP\_TRAY\_ICON\_GUID;  
    StringCchCopyW(nid.szTip, ARRAYSIZE(nid.szTip), tooltipText);

    return Shell\_NotifyIconW(NIM\_MODIFY, \&nid) ? S\_OK : E\_FAIL;  
}

// トレイアイコンの削除（アプリケーション終了時）  
HRESULT UnregisterTrayIcon()  
{  
    NOTIFYICONDATAW nid \= {};  
    nid.cbSize \= sizeof(NOTIFYICONDATAW);  
    nid.uFlags \= NIF\_GUID;  
    nid.guidItem \= APP\_TRAY\_ICON\_GUID;

    return Shell\_NotifyIconW(NIM\_DELETE, \&nid) ? S\_OK : E\_FAIL;  
}

Explorerプロセスの予期せぬクラッシュや再起動に対しては、RegisterWindowMessage(L"TaskbarCreated") により取得したブロードキャストメッセージをウィンドウプロシージャで監視します。タスクバー再構築の通知を受信した際に RegisterTrayIcon を再実行することで、タスクバー上にアイコンを復元できます。固定GUIDを使用しているため、この再登録処理によって設定画面側のエントリが増殖することはなく、ユーザーのピン留め設定も完全に維持されます2。

## **6\. 開発者向け推奨事項の総括**

Windows 11の「その他のシステムトレイアイコン」画面において健全な表示状態を維持するための要件は、以下の原則に集約されます。  
実行ファイルの配置パスを固定化し、インプレースアップデートを基本設計とすることで、シェルの識別データベースにおける不要な新規エントリ生成を防ぎます2。更新に伴うパス変更が不可避なアーキテクチャでは、同一企業によるAuthenticode電子署名を施すことで、シェルの自動設定移行機能を有効化します2。  
アイコン識別には NOTIFYICONDATAW による固定GUID（NIF\_GUID）および NOTIFYICON\_VERSION\_4 を採用し、DPI対応には LoadIconMetric を利用します1。設定画面に表示される名称と発行元の整合性を確保するために、バイナリリソース内の FileDescription および CompanyName を正確に定義し、プロセス終了時の NIM\_DELETE の実行とアンインストール時のレジストリ清掃を徹底することが推奨されます1。

#### **引用文献**

> 1. Notifications and the Notification Area \- Win32 apps | Microsoft Learn, [https://learn.microsoft.com/en-us/windows/win32/shell/notification-area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area)  
> 2. NOTIFYICONDATAA structure (shellapi.h) \- Win32 \- Microsoft Learn, [https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataa](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataa)  
> 3. "Show all icons in system tray" option in windows11 | Microsoft Community Hub, [https://techcommunity.microsoft.com/discussions/windows11/show-all-icons-in-system-tray-option-in-windows11/3359877/replies/4077600](https://techcommunity.microsoft.com/discussions/windows11/show-all-icons-in-system-tray-option-in-windows11/3359877/replies/4077600)  
> 4. Where is the Icon Tray (notification area) registry stored? \- Super User, [https://superuser.com/questions/1332399/where-is-the-icon-tray-notification-area-registry-stored](https://superuser.com/questions/1332399/where-is-the-icon-tray-notification-area-registry-stored)  
> 5. How to delete duplicate System Tray Icon list from Windows 11 Settings? \- Microsoft Learn, [https://learn.microsoft.com/en-us/answers/questions/4283039/how-to-delete-duplicate-system-tray-icon-list-from](https://learn.microsoft.com/en-us/answers/questions/4283039/how-to-delete-duplicate-system-tray-icon-list-from)  
> 6. Monitorian has multiple 'Tray icon preference' entries in Registry key IconStreams \#154, [https://github.com/emoacht/Monitorian/issues/154](https://github.com/emoacht/Monitorian/issues/154)  
> 7. How to Clear Customize Notifications (Taskbar Corner Overflow, [https://www.winhelponline.com/blog/clear-customize-notifications-tray-items-windows/](https://www.winhelponline.com/blog/clear-customize-notifications-tray-items-windows/)  
> 8. Trying to make an application in the system tray show up in the active window for all machines | Page 2, [https://www.elevenforum.com/t/trying-to-make-an-application-in-the-system-tray-show-up-in-the-active-window-for-all-machines.19897/page-2](https://www.elevenforum.com/t/trying-to-make-an-application-in-the-system-tray-show-up-in-the-active-window-for-all-machines.19897/page-2)  
> 9. Organize tray icons of Windows 11 \- GitHub Gist, [https://gist.github.com/foriequal0/7ef88eb7c47bee6da696a426e8a00df6](https://gist.github.com/foriequal0/7ef88eb7c47bee6da696a426e8a00df6)  
> 10. Winapi Shell\_NotifyIcon \- Duplicated icons in Notification Area Icons window, [https://stackoverflow.com/questions/16673663/winapi-shell-notifyicon-duplicated-icons-in-notification-area-icons-window](https://stackoverflow.com/questions/16673663/winapi-shell-notifyicon-duplicated-icons-in-notification-area-icons-window)  
> 11. How delete icon in taskbar settings main panel? \- Microsoft Learn, [https://learn.microsoft.com/en-us/answers/questions/3894671/how-delete-icon-in-taskbar-settings-main-panel](https://learn.microsoft.com/en-us/answers/questions/3894671/how-delete-icon-in-taskbar-settings-main-panel)  
> 12. Two AIP task tray icons appear in Windows Taskbar Setting \- Actiphy.com, [https://actiphyhelp.zendesk.com/hc/en-us/articles/41226256832665-Two-AIP-task-tray-icons-appear-in-Windows-Taskbar-Setting](https://actiphyhelp.zendesk.com/hc/en-us/articles/41226256832665-Two-AIP-task-tray-icons-appear-in-Windows-Taskbar-Setting)  
> 13. Unhide specific system tray icon : r/Intune \- Reddit, [https://www.reddit.com/r/Intune/comments/1cz5kh4/unhide\_specific\_system\_tray\_icon/](https://www.reddit.com/r/Intune/comments/1cz5kh4/unhide_specific_system_tray_icon/)  
> 14. Using NotifyIcon in WinUI 3 | Albert Akhmetov, [https://albertakhmetov.com/posts/2025/using-notifyicon-in-winui-3/](https://albertakhmetov.com/posts/2025/using-notifyicon-in-winui-3/)  
> 15. Hello guys, does anyone know how could i remove these tray icons from the settings app? : r/Windows11 \- Reddit, [https://www.reddit.com/r/Windows11/comments/1hwtyrp/hello\_guys\_does\_anyone\_know\_how\_could\_i\_remove/](https://www.reddit.com/r/Windows11/comments/1hwtyrp/hello_guys_does_anyone_know_how_could_i_remove/)  
> 16. Proper Way of Handling GUID used by Windows NotificationIcons \- Stack Overflow, [https://stackoverflow.com/questions/73334917/proper-way-of-handling-guid-used-by-windows-notificationicons](https://stackoverflow.com/questions/73334917/proper-way-of-handling-guid-used-by-windows-notificationicons)  
> 17. system tray \- NOTIFYICONDATA \- GUID problem \- Stack Overflow, [https://stackoverflow.com/questions/7432319/notifyicondata-guid-problem](https://stackoverflow.com/questions/7432319/notifyicondata-guid-problem)  
> 18. EarTrumpet doesn't stay in notification area between sessions · Issue \#272 \- GitHub, [https://github.com/File-New-Project/EarTrumpet/issues/272](https://github.com/File-New-Project/EarTrumpet/issues/272)  
> 19. Windows-classic-samples/Samples/Win7Samples/winui/shell/appshellintegration/NotificationIcon/NotificationIcon.cpp at main · microsoft/Windows-classic-samples \- GitHub, [https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/Win7Samples/winui/shell/appshellintegration/NotificationIcon/NotificationIcon.cpp](https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/Win7Samples/winui/shell/appshellintegration/NotificationIcon/NotificationIcon.cpp)