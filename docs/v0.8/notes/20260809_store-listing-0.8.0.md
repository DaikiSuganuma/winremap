# Microsoft Store 掲載情報の差分（v0.8.0）

- 作成日: 2026-08-09
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／提出はオーナー
- 元資料: [v0.6.0 の掲載情報草案](../../v0.6/notes/20260729_store-listing.md)（**説明・機能・検索キーワード・スクリーンショット・`runFullTrust` の理由説明はそのまま使える**）、[v0.7.0 の差分ノート](../../v0.7/notes/20260802_store-listing-0.7.0.md)、[リリース手順 §4.2](../../03_release-operations.md)
- 関連: [ADR 0067](../decisions/0067-ime-cursor-color.md)（IME をマウスカーソルの色で示す）、[ADR 0068](../decisions/0068-debug-console.md)（`--debug` の専用コンソール）

## 0. この文書の範囲

**掲載情報のほとんどは v0.6.0 のまま出せる。** 変えるのは 2 か所だけである（v0.7.0 と同じ）。

| 欄 | 扱い |
|---|---|
| **このバージョンの新機能 / What's new in this version** | **毎回書き換える。** §1 の文面を使う |
| **説明 / Description** の「主な機能」 | **1 行足す**（IME のカーソル）。§2 |
| 短い説明・機能・検索キーワード・スクリーンショット | **変更なし** |
| `runFullTrust` の理由説明 | **変更なし**（[v0.6 草案 §3](../../v0.6/notes/20260729_store-listing.md)） |

## 1. このバージョンの新機能（提出フォームに貼る）

### 日本語

> IME がオンの間、マウスカーソルに色を付けられるようになりました（既定は無効）。一瞬表示される「あ」パネルと違い、見たいときにいつでも入力モードを確かめられます。あわせて `--debug` が専用のコンソールウィンドウを開くようになり、起動時のログが 1 行目から、終了時のログも読めるようになりました。

### English

> While the IME is on, the mouse cursor can take a colour of your choosing (off by default). Unlike the 「あ」 panel, which flashes and is gone, it tells you the input mode whenever you look. `--debug` now opens a console window of its own, so the startup lines can be read from the first one and the shutdown lines no longer vanish with the process.

## 2. 「主な機能」に足す 1 行

日本語の箇条書き（[v0.6 草案 §1](../../v0.6/notes/20260729_store-listing.md) の「主な機能」）に、IME インジケーターの行の次へ:

> ・IME がオンの間、マウスカーソルの色でも入力モードが分かります（既定は無効）

English（the IME indicator bullet の次）:

> - While the IME is on, the mouse cursor can show the input mode too (off by default)

**「既知の制限」の行は変えない。** 管理者権限ウィンドウ（UIPI）の制限は今回も変わっていない。カーソルの色も同じ理由で昇格した窓の上では追随しないが、これは既存の 1 行が読める範囲である。

## 3. 提出時の注意（前回からの引き継ぎ）

- **署名しない。** Store が再署名する（[リリース手順 §4.2](../../03_release-operations.md)）
- **バージョンは手で触らない。** `build.ps1` が `Cargo.toml` から読んで `0.8.0.0` を埋める
- **提出は GitHub Release の公開後。** 同じソース・同じタグという約束のため（**2026-08-09 に公開済み**）
- **自動更新される。** 壊れた版を出すと Store 利用者全員に自動で届く

## 4. この版だけの注意 — **認定通過の連絡が来た日が、P-9 を測れる最後の日**

v0.7.0 では、認定通過（2026-08-04）から受け入れの実施までのあいだに **Store 版が自動で 0.7.0 へ更新され**、`0.6.0 → 0.7.0` の更新経路（P-9）は**永久に測れなくなった**（[v0.7 §7](../../v0.7/03_acceptance-checklist.md)）。

**0.8.0 でも同じことが起きる。** 通過の連絡が来たら、その場で:

1. **P-9 を先に**（`0.7.0` が入っている状態で「ライブラリ → 更新」）
2. そのあと**アンインストールして P-8**（Store から新規インストール。SmartScreen が出ないこと）

逆順にすると P-9 の前提が消える。**この 1 回きりである。**
