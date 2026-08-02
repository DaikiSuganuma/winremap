# Microsoft Store 掲載情報の差分（v0.7.0）

- 作成日: 2026-08-01
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／提出はオーナー
- 元資料: [v0.6.0 の掲載情報草案](../../v0.6/notes/20260729_store-listing.md)（**説明・機能・検索キーワード・スクリーンショット・`runFullTrust` の理由説明はそのまま使える**）、[リリース手順 §4.2](../../03_release-operations.md)
- 関連: [ADR 0063](../decisions/0063-symbol-keys.md)（記号キー）、[ADR 0065](../decisions/0065-foreground-window-from-the-event.md)（前面アプリの取り違え）

## 0. この文書の範囲

**掲載情報のほとんどは v0.6.0 のまま出せる。** 変える必要があるのは 2 か所だけである。

| 欄 | 扱い |
|---|---|
| **このバージョンの新機能 / What's new in this version** | **毎回書き換える。** §1 の文面を使う |
| **説明 / Description** の「主な機能」 | **1 行足す**（記号キー）。§2 |
| 短い説明・機能・検索キーワード・スクリーンショット | **変更なし** |
| `runFullTrust` の理由説明 | **変更なし**（[v0.6 草案 §3](../../v0.6/notes/20260729_store-listing.md)） |

## 1. このバージョンの新機能（提出フォームに貼る）

### 日本語

> `;` や `/` などの記号キーを設定に書けるようになりました。キーボードの刻印どおりに書けば、その機械の配列に合わせて解釈されます（配列によらない `Oem1` 等の名前も使えます）。あわせて、アプリを切り替えた直後に前のアプリのルールが使われることがあった不具合を修正しました。

### English

> Punctuation and symbol keys such as `;` and `/` can now be used in rules. Write them the way your keyboard is engraved and WinRemap resolves them for that layout; layout-independent names (`Oem1` and friends) are accepted too. This release also fixes a bug where the rules of the previous application could still be applied right after switching apps.

## 2. 「主な機能」に足す 1 行

日本語の箇条書き（[v0.6 草案 §1](../../v0.6/notes/20260729_store-listing.md) の「主な機能」）に、`・アプリ別のリマップ` の次あたりへ:

> ・記号キー（`;` `/` `@` など）もリマップ対象。キーボードの刻印どおりに書けます

English（`Per-application remapping` の次）:

> - Punctuation and symbol keys (`;`, `/`, `@`) can be remapped too, written the way your keyboard is engraved

**「既知の制限」の行は変えない。** 管理者権限ウィンドウ（UIPI）の制限は今回も変わっていない。

## 3. 提出時の注意（前回からの引き継ぎ）

- **署名しない。** Store が再署名する（[リリース手順 §4.2](../../03_release-operations.md)）
- **バージョンは手で触らない。** `build.ps1` が `Cargo.toml` から読んで `0.7.0.0` を埋める
- **提出は GitHub Release の公開後。** 同じソース・同じタグという約束のため
- **自動更新される。** 壊れた版を出すと Store 利用者全員に自動で届く。受け入れを**リリースビルドで**通してから提出する
