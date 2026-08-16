# Microsoft Store 掲載情報の差分（v1.0.0）

- 作成日: 2026-08-16
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／提出はオーナー
- 元資料: [v0.6.0 の掲載情報草案](../../v0.6/notes/20260729_store-listing.md)（**説明・機能・検索キーワード・スクリーンショット・`runFullTrust` の理由説明はそのまま使える**）、[v0.9.0 の差分ノート](../../v0.9/notes/20260814_store-listing-0.9.0.md)、[リリース手順 §4.2](../../03_release-operations.md)
- 関連: [ADR 0076](../decisions/0076-read-cursors-unscaled.md)（拡大表示でカーソルを読む）、[ADR 0077](../decisions/0077-remember-the-chosen-config-file.md)（設定ファイルの引き継ぎ）、[ADR 0079](../decisions/0079-tray-menu-names-the-config-file.md)（トレイに設定ファイル名）
- 公式参照: [Store 提出時のパッケージ要件](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)

## 0. この文書の範囲

**掲載情報のほとんどは v0.6.0 のまま出せる。この版で書き換えるのは 1 か所だけである。**

| 欄 | 扱い |
|---|---|
| **このバージョンの新機能 / What's new in this version** | **書き換える。** §1 の文面を使う |
| **説明 / Description** の「主な機能」 | **変更なし** |
| 機能・短い説明・検索キーワード・スクリーンショット | **変更なし** |
| `runFullTrust` の理由説明 | **変更なし**（[v0.6 草案 §3](../../v0.6/notes/20260729_store-listing.md)） |

**「主な機能」に足す行は無い。** 増えた 2 つ（設定ファイルの引き継ぎ・トレイのメニューの表示）は、いずれも既にある「設定ウィンドウで、いま効いている設定の確認とその場での編集」の延長であり、新しい機能の柱ではない。

> **`git diff v0.9.0..HEAD -- src\config\raw.rs` が空である。** 設定名を受け付ける構造体が 1 行も変わっていない ＝ **利用者が書ける設定は 1 つも増えていない**（`last-config.txt` は設定ではなく、アプリが書く 1 行の記録である）。

## 1. このバージョンの新機能（提出フォームに貼る）

### 日本語

> 使っていて気づいたことを直した版です。バージョンが 1.0 になりましたが、これは何かを凍結する宣言ではなく、いまの機能で作者自身が使い続けて満足している、という表明です。
>
> ・**画面の拡大率が 125% 以上のとき、文字入力のカーソル（I ビーム）に IME の色が付かなかった問題を直しました。**矢印には付くのに I ビームには付かない、という形で出ていました
> ・**前回選んだ設定ファイルで起動するようになりました。**設定ウィンドウで別の `.toml` に切り替えると、次の起動でもそのファイルを開きます
> ・**タスクトレイのメニューに、いま読み込んでいる設定ファイルの名前が出るようになりました**
> ・設定フォルダーのパスが長いとき、設定ウィンドウのボタンがウィンドウの外へ出てしまう問題を直しました。**このアプリのようにパッケージ専用フォルダーを使う場合に起きていました**

### English

> A release of things noticed while using it. The version number is now 1.0, which is not a freeze — it says the feature set is one its author is happy to keep using.
>
> - **Fixed: on a display scaled to 125% or more, the text cursor (I-beam) did not take the IME colour.** The arrow was tinted and the I-beam was not
> - **WinRemap now opens the config file you chose last time.** Switch to another `.toml` in the settings window and the next start opens that one
> - **The tray menu now names the config file in force**
> - Fixed: a long config folder path pushed the settings window's buttons off the window — **which is what a packaged app's own folder does**

## 2. 提出前の確認

**提出するのは、Release を公開したあとにタグの内容から作り直したものである** — §4.1 の順序制約であり、タグと中身を一致させるためでもある。下の表は**その作り直した提出物**に対する結果で、**2026-08-16 07:24 UTC に `main`（`34cf7b8` ＝ `v1.0.0`）で `build.ps1 -Pack` を回した**もの。

| 見るもの | 期待 | 提出物の結果 |
|---|---|---|
| `AppxSignature.p7x` | **入っていない**（Store が再署名する） | **入っていない** ✓ |
| `Identity/@Name` | `SUGANUMADaiki.WinRemap` | 一致 ✓ |
| `Identity/@Version` | **`1.0.0.0`**（第 4 フィールドは Store 用で常に 0） | **`1.0.0.0`** ✓ |
| `Identity/@Publisher` | `CN=38CDEE8D-0FAC-4CBA-A3DA-17BBDD107F55` | 一致 ✓ |
| `Identity/@ProcessorArchitecture` | `x64` | 一致 ✓ |
| `PublisherDisplayName` | `SUGANUMA Daiki` | 一致 ✓ |
| ファイル | `packaging\msix\out\winremap-1.0.0.msix` | **4,304,474 バイト、SHA256 `3fa75c1d01bf5b1963d435b2d247226bb04afeb94f5d323c7bef5db98cc0ca15`** |
| 中の `winremap.exe` | 手元の配布ビルドと同じ | 9,884,160 バイト（一致） |
| `resources.pri` | **入っている**（0.9.0 のアイコン修正。無いと unplated アセットが引かれない） | 3,664 バイト・`altform-unplated` × 6 ✓ |
| §4.1 の順序制約 | GitHub Release `v1.0.0` が公開済み、`privacy.html` が日英とも 200 | **満たした** ✓（公開 07:13 UTC、`privacy.html` は日英とも 200） |

> **同じソースでも再ビルドでファイルは変わる。** 公開前に一度作ったもの（4,304,467 バイト・SHA256 `aaee7a4f…`）とは**別のファイル**である。**提出するのは上の表の方**（`3fa75c1d…`）。中の `winremap.exe` は 9,884,160 バイトで両者とも同じである。

## 3. 提出時の注意（前回からの引き継ぎ）

- **署名しない。** Store が再署名する
- **バージョンは手で触らない。** `build.ps1` が `Cargo.toml` から読んで `1.0.0.0` を埋める
- **提出は GitHub Release の公開後。** 同じソース・同じタグという約束のため
- **自動更新される。** 壊れた版を出すと Store 利用者全員に自動で届く

## 4. この版の判断 — **P-9 は測る**……はずだったが、**捨てた**

> **⚠ 2026-08-16 追記（実施後）。この節の計画は実行されていない。**
>
> **P-9 は測れない。更新元の Store 版 0.9.0 を削除したためである。** 受け入れの P 区切りを認定通過を待たずに回すことになり、P-1〜P-2 の前準備がアンインストールから始まる以上、0.9.0 を残す方法が無かった。**Store は旧バージョンを配らないので復元できない。** オーナーがこの代償を承知のうえで「今すぐ回す」を選んだ（[受け入れ §9](../03_acceptance-checklist.md) の 3 回目の記録）。
>
> **したがって認定通過後にやることは P-8 だけである。** §7.2 の「P-9 → P-8 の順序」は、この版に限って気にしなくてよい。**下に書いてある「更新前に設定ファイルを切り替えておく」「自動起動をオンにしておく」も不要**である。
>
> **次の版で、1.0.0 が Store に載った状態から次版への更新として測ること。** そのとき見るのは、下の表の①「`last-config.txt` が更新をまたいで残り、切り替えたファイルで起動すること」である — **この版で増えた観点は、次の版へそのまま持ち越す。**

以下は、この節を書いた時点（提出前）の計画である。**次の版の P-9 で使う。**

v0.9.0 では「P-9 が測るものを 1 つも変えていない」として未実施にした。**この版は違う。**

**P-9 の通過条件①「設定ファイルがそのまま残る」の中身が増えている。** [ADR 0077](../decisions/0077-remember-the-chosen-config-file.md) で `last-config.txt` が加わり、**更新をまたいで残らなければ、利用者は「昨日と違う設定で立ち上がった」を体験する** — この版が消そうとしている混乱そのものである。

| P-9 の通過条件 | この版での状態 |
|---|---|
| ①設定ファイルが残る | **新しい観点。** `config.toml` に加えて **`last-config.txt` が残り、切り替えたファイルで起動すること** |
| ②表示が 1.0.0 になる | 仕組みは変更なし（`build.ps1` が `Cargo.toml` から埋める） |
| ③旧版が残らない | プラットフォーム側。変更なし |
| ④自動起動の設定が保たれる | `windows.startupTask`。**マニフェストに差分なし**（版番号だけ） |

**順序の制約は従来どおり。** 認定通過の連絡を見たら、**他の何よりも先に P-9（0.9.0 → 1.0.0 の更新）を回す**。P-8（アンインストール → Store から新規インストール）を先にやると、更新元の 0.9.0 が消えて二度と測れない。**更新の前に、設定ウィンドウで `config.toml` 以外のファイルへ切り替えておくこと** — 既定のままでは①の新しい観点が測れない。
