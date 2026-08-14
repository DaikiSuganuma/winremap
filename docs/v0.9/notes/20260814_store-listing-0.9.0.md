# Microsoft Store 掲載情報の差分（v0.9.0）

- 作成日: 2026-08-14
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／提出はオーナー
- 元資料: [v0.6.0 の掲載情報草案](../../v0.6/notes/20260729_store-listing.md)（**説明・機能・検索キーワード・スクリーンショット・`runFullTrust` の理由説明はそのまま使える**）、[v0.7.0 の差分ノート](../../v0.7/notes/20260802_store-listing-0.7.0.md)、[v0.8.0 の差分ノート](../../v0.8/notes/20260809_store-listing-0.8.0.md)、[リリース手順 §4.2](../../03_release-operations.md)
- 関連: [ADR 0073](../decisions/0073-restore-from-a-snapshot-taken-at-startup.md)（起動時の複製から戻す）、[ADR 0075](../decisions/0075-a-half-installed-tint-must-say-so.md)（半分だけの着色を報告する）
- 公式参照: [Store 提出時のパッケージ要件](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)、[パッケージ マニフェスト スキーマ（VisualElements）](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap-visualelements)

## 0. この文書の範囲

**掲載情報のほとんどは v0.6.0 のまま出せる。この版で書き換えるのは 1 か所だけである。**

| 欄 | 扱い |
|---|---|
| **このバージョンの新機能 / What's new in this version** | **書き換える。** §1 の文面を使う |
| **説明 / Description** の「主な機能」 | **変更なし** |
| 機能・短い説明・検索キーワード・スクリーンショット | **変更なし** |
| `runFullTrust` の理由説明 | **変更なし**（[v0.6 草案 §3](../../v0.6/notes/20260729_store-listing.md)） |

**v0.8.0 と違って「主な機能」に足す行が無い。** 0.9.0 は利用者から見える新機能をほとんど持たない修正版であり、設定ウィンドウでカーソルの色を編集できるようになった点も、既存の「設定ウィンドウで、いま効いている設定の確認とその場での編集」に収まる。

> **`git diff v0.8.0..HEAD -- src\config\raw.rs` が空である。** 設定名を受け付ける構造体が 1 行も変わっていない ＝ **利用者から見える設定は 1 つも増えていない**。掲載情報の機能欄を触らない根拠でもある。

## 1. このバージョンの新機能（提出フォームに貼る）

### 日本語

> 0.8.0 で入れた「IME がオンの間、マウスカーソルに色が付く」機能の修正版です。
>
> ・色が付かなくなる／戻らなくなることがあった問題に手を当てました。中身が空のカーソルは組み込まれなくなり、以前あった「IME を切り替えても戻らない」状態にはなりません
> ・着色できなかったカーソルがあるときは、その数と理由が `--debug` のログに残るようになりました。これまでは片方だけ着いても成功として報告されていました
> ・終了時にカーソルを元へ戻す処理が、実際に働くようになりました
> ・設定ウィンドウからカーソルの色を編集できるようになりました
> ・**このアプリのアイコンが正しく表示されるようになりました。**これまでは青い四角に見えていました（タスクバー・ウィンドウ・設定 → アプリ → スタートアップ）

### English

> A fix release for the coloured cursor introduced in 0.8.0.
>
> - The cases where the colour stopped being drawn, or would not come back, have been addressed. A cursor with nothing drawn in it is never installed, so the old form — where toggling the IME did not bring it back — no longer happens
> - When a cursor cannot be tinted, the count and the reason are now written to the `--debug` log. Until now a tint that only half went on was reported as a success
> - Restoring the cursor on exit works now
> - The cursor colour can be edited from the settings window
> - **This app's icon is displayed correctly.** Until now it appeared as a solid blue square (taskbar, window, and Settings → Apps → Startup)

## 2. 提出前の確認（2026-08-14 に済ませてある）

| 見たもの | 結果 |
|---|---|
| `AppxSignature.p7x` | **入っていない**（意図どおり。Store が再署名する） |
| `Identity/@Name` | `SUGANUMADaiki.WinRemap` |
| `Identity/@Version` | **`0.9.0.0`**（第 4 フィールドは Store 用で常に 0） |
| `Identity/@Publisher` | `CN=38CDEE8D-0FAC-4CBA-A3DA-17BBDD107F55` |
| `Identity/@ProcessorArchitecture` | `x64` |
| `PublisherDisplayName` | `SUGANUMA Daiki` |
| `VisualElements/@BackgroundColor` | **`#FFFFFF`**（この版の変更点） |
| ファイル | `packaging\msix\out\winremap-0.9.0.msix`、4,299,591 バイト、SHA256 `06ac0496987b6a50bb143e25e7913415aa066b40ac4fd0e34e46a7e1ed9eba16` |
| 中の `winremap.exe` | `e2d45f6a4d5e787229b287895b9c6fa09783830c5313fa7d1f2fb1dcdb6afcce`（手元の配布ビルドと一致） |
| §4.1 の順序制約 | **満たしている** — GitHub Release `v0.9.0` は 2026-08-14 02:42 UTC に公開済み、`privacy.html` は日英とも 200 |

> パッケージ内の exe は、GitHub Releases の `winremap.exe`（`fe804e4a…`）とはバイト列が違う。**同じソース・同じタグから、違う機械で建てた**ためである（SECURITY.md の約束は「同じソース・同じタグ」であって、同一バイトではない）。

## 3. 提出時の注意（前回からの引き継ぎ）

- **署名しない。** Store が再署名する（[リリース手順 §4.2](../../03_release-operations.md)）
- **バージョンは手で触らない。** `build.ps1` が `Cargo.toml` から読んで `0.9.0.0` を埋める
- **提出は GitHub Release の公開後。** 同じソース・同じタグという約束のため（**2026-08-14 に公開済み**）
- **自動更新される。** 壊れた版を出すと Store 利用者全員に自動で届く

## 4. この版の判断 — **P-9 は測らない。P-8 は薦める**

v0.7.0・v0.8.0 の差分ノートは、この節で「認定通過の連絡が来た日が P-9 を測れる最後の日」と急かしてきた。**0.9.0 ではその急ぎが要らない。**

### P-9（更新 0.8.0 → 0.9.0）— 未実施にする

**この版は、P-9 が測るものを 1 つも変えていない。**

| P-9 の通過条件 | 実装のありか | `v0.8.0..HEAD` |
|---|---|---|
| ①設定ファイルが残る | パッケージのデータ領域（プラットフォーム側）・`src/package.rs` | **変更なし** |
| ②表示が 0.9.0 になる | `Identity/@Version`（`build.ps1` が埋める） | 仕組みは**変更なし** |
| ③旧版が残らない | MSIX の更新機構（プラットフォーム側） | — |
| ④自動起動の設定が保たれる | `windows.startupTask` | **変更なし**（マニフェストの差分は `BackgroundColor` の 1 行だけ） |

**v0.8.0 で同じ経路を測って通過している**（0.7.0 → 0.8.0、[v0.8 §7](../../v0.8/03_acceptance-checklist.md)）。**測り直す理由が無い。**

> **代償ははっきりさせておく。** Store は旧バージョンを配らないので、**`0.8.0 → 0.9.0` の更新経路は、これで永久に測れない**。v0.7.0 で失ったのと同じ種類の穴が 1 つ増える。ただしあちらは**気づかないうちに失った**のに対し、こちらは**変わっていないと分かったうえで測らない**という判断である。
>
> これを測るためだけに Store 版 0.8.0 を入れ直す必要は無い、というのがこの版の結論である。**入れ直しは、認定の連絡を待って他の作業より先に更新を回す**という時間の制約も連れてくる（管理者権限が無いと Store の自動更新は切れない）。**変わっていないものを測るために、その制約を背負わない。**

### P-8（Store 経由インストール）— 認定通過後に実施を薦める

**こちらは、この版に固有の測る値打ちがある。**

①SmartScreen が出ない ②発行元が `SUGANUMA Daiki` ③初回起動が P-1 と同じ、の 3 条件は確かに変わっていない。**薦める理由は 4 つ目にある** — **0.9.0 のアイコン修正が、Store 署名版でまだ一度も見られていない**。

P-10 は 2026-08-14 に通過しているが、測ったのは **`build.ps1 -Register` で登録した loose layout** である。Store は受け取った `.msix` を再署名して配るので、**利用者に届く形での確認はまだ無い**。

そして、この不具合の教訓がまさにそこにあった — **「パッケージにファイルを置くこと」と「それが使われること」は別**である（`resources.pri` が無いあいだ、アセットは入っていたのに一度も引かれなかった）。**同じ種類の思い込みを、配布経路の側で繰り返さないための 3 分**である。

**手順（所要 3 分）**

1. 認定通過後、ストアページから 0.9.0 をインストールする（**0.8.0 を先に入れ直す必要は無い**）
2. ①SmartScreen の警告が出ないこと ②発行元が `SUGANUMA Daiki` であること
3. **アイコンを 3 面で見る** — タスクバー・ウィンドウ・設定 → アプリ → スタートアップ。**青い四角になっていないこと**
4. トレイに常駐し、メモ帳で `C-h` が 1 文字削除になること
