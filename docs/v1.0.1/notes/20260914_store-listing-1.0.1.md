# Microsoft Store 掲載情報の差分（v1.0.1）

- 作成日: 2026-09-14
- 作成: Claude Code（AI モデル: claude-opus-5）／提出はオーナー
- 元資料: [v0.6.0 の掲載情報草案](../../v0.6/notes/20260729_store-listing.md)、[v1.0.0 の差分ノート](../../v1.0/notes/20260816_store-listing-1.0.0.md)、[リリース手順 §4.2](../../03_release-operations.md)
- 関連: [ADR 0080](../decisions/0080-tray-icon-asks-for-the-small-metric.md)・[ADR 0081](../decisions/0081-icon-must-not-depend-on-its-background.md)・[ADR 0082](../decisions/0082-small-master-for-small-sizes.md)、[受け入れチェックリスト](../03_acceptance-checklist.md)
- 公式参照: [Store 提出時のパッケージ要件](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)

## 0. この文書の範囲

**書き換えるのは「このバージョンの新機能」の 1 欄だけである。**

| 欄 | 扱い |
|---|---|
| **このバージョンの新機能 / What's new in this version** | **書き換える。** §1 の文面を使う |
| 説明・主な機能・短い説明・検索キーワード・スクリーンショット | 変更なし |
| `runFullTrust` の理由説明 | 変更なし（[v0.6 草案 §3](../../v0.6/notes/20260729_store-listing.md)） |

`git diff v1.0.0..HEAD -- src\config\raw.rs` は空で、利用者が書ける設定は増えていない。`AppxManifest.xml` にも差分は無い（版番号は `build.ps1` が `Cargo.toml` から埋める）。パッケージ用の画像 30 枚は焼き直している。

## 1. このバージョンの新機能（提出フォームに貼る）

### 日本語

> 1.0.0 のリリース後に気づいた、タスクトレイまわりの 2 件を直した版です。
>
> ・**設定アプリの「その他のシステム トレイ アイコン」の一覧で、アイコンが青い四角に見えていた問題を直しました。**キーを白で塗る形に描き直し、明るいタスクバーでも暗いタスクバーでも読めるようにしました
> ・タスクトレイのアイコンが縮小でぼやけていた問題を直しました
> ・タスク マネージャーなどで、アプリ名が小文字の「winremap」と表示されていた問題を直しました

### English

> Two fixes to the tray, noticed after 1.0.0.
>
> - **Fixed: the icon showed as a blue square in Settings' "Other system tray icons" list.** The keys are now painted white, so the icon reads on a light taskbar and a dark one
> - Fixed: the tray icon was blurred by rescaling
> - Fixed: Task Manager and other places showed the app's name as a lower-case "winremap"

## 2. 提出前の確認

**提出するのは、Release を公開したあとにタグの内容から作り直したものである**（§4.1 の順序制約）。下の表は、**2026-09-14 05:51 UTC に `v1.0.1`（`1708261`）を git worktree へ取り出し、そこで `build.ps1 -Pack` を回した**結果である。worktree を使ったのは、開発者登録した 1.0.1 が本体の `packaging\msix\layout` から動いていたためで、そのフォルダーに触れずに作った。

| 見るもの | 期待 | 提出物の結果 |
|---|---|---|
| `AppxSignature.p7x` | 入っていない（Store が再署名する） | 入っていない ✓ |
| `Identity/@Name` | `SUGANUMADaiki.WinRemap` | 一致 ✓ |
| `Identity/@Version` | `1.0.1.0` | `1.0.1.0` ✓ |
| `Identity/@Publisher` | `CN=38CDEE8D-0FAC-4CBA-A3DA-17BBDD107F55` | 一致 ✓ |
| `Identity/@ProcessorArchitecture` | `x64` | 一致 ✓ |
| `PublisherDisplayName` | `SUGANUMA Daiki` | 一致 ✓ |
| ファイル | `packaging\msix\out\winremap-1.0.1.msix` | 4,320,420 バイト、SHA256 `e3477060ccc9381e0d654d846e47789ef5b4c9cca03ce4d97ecc3092155e88e3` |
| 中の `winremap.exe` | タグから作った配布ビルド | 9,956,864 バイト（worktree の `target\release\winremap.exe` と同じ） |
| `resources.pri` | 入っている・`altform-unplated` × 6 | 3,664 バイト・`altform-unplated` × 6 ✓。`resources.scale-{125,150,200,400}.pri` も入っており、エントリ数 39 は 1.0.0 の提出物と同じ |
| §4.1 の順序制約 | GitHub Release `v1.0.1` が公開済み、`privacy.html` が日英とも 200 | 満たした ✓（公開 05:46:46 UTC、`privacy.html` は日英とも 200） |

中の `winremap.exe`（9,956,864 バイト）は、GitHub Release の `winremap.exe`（9,935,872 バイト、CI でビルド）とはバイト数が違う。ビルドした機械が違うためで、ソースはどちらも `v1.0.1` である。

## 3. 提出時の注意

- **署名しない。** Store が再署名する
- **バージョンは手で触らない。** `build.ps1` が `Cargo.toml` から読んで `1.0.1.0` を埋める
- **提出は GitHub Release の公開後**
- **`build.ps1` は `packaging\msix\layout` を作り直す。** 2026-09-14 の受け入れで開発者登録した 1.0.1 のインストール先がこのフォルダーなので、本体で `-Pack` を回すと登録した版は動かなくなる。今回は §2 のとおり worktree で作った

**2026-09-14、オーナーが §2 の提出物を Partner Center へ提出した。認定待ち。**

## 4. 認定通過後にやること

- **P-8**（Store から新規インストール）。この機械の Store 版 1.0.0 は、受け入れの P 区切りで削除した（[受け入れ §5](../03_acceptance-checklist.md)）。P-8 の前に、開発者登録した 1.0.1 を削除する必要がある。**削除するとパッケージ専用フォルダーの `LocalCache\Roaming\winremap`（オーナーの `personal-ja.toml` と `last-config.txt` を戻してある）も消えるので、先に退避すること**
- **P-9 は測れない。** 更新元の Store 版 1.0.0 が無いため。次の版で、Store 版 1.0.1 からの更新として測る
