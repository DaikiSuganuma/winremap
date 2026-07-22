# packaging/ — パッケージマネージャー用マニフェスト

- 作成日: 2026-07-22
- 作成: Claude Code（AI モデル: claude-opus-4-8）／レビュー・承認: オーナー

WinRemap を winget / scoop から入れられるようにするためのマニフェスト置き場。方針は
[ADR 0045](../docs/v0.3/decisions/0045-package-manager-channels.md) を参照。**いずれも
公式 GitHub Releases の資産（URL と SHA256）を指す**ため、他サイト配布とは別物の「公式
Releases への別の入口」である。

マニフェストは各バージョンのリリース後に、その資産の URL とハッシュを埋めて更新する
（[docs/03_release-operations.md](../docs/03_release-operations.md) §4）。ここに置くのは
提出物の control copy であり、実際に配信されるのは下記の各リポジトリ側のコピーである。

## winget（`winget/`）

- パッケージ ID: `DaikiSuganuma.WinRemap`（`Moniker: winremap`）
- 3 ファイル構成（installer / defaultLocale / version、スキーマ 1.6.0）
- 提出先: [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) の
  `manifests/d/DaikiSuganuma/WinRemap/<version>/` へ PR

ローカル検証（Windows Package Manager が入っていること）:

```powershell
winget validate --manifest packaging\winget
# 実機導入テスト（ローカルマニフェストの許可が必要）
winget settings --enable LocalManifestFiles
winget install --manifest packaging\winget
```

提出は [wingetcreate](https://github.com/microsoft/winget-create) を使うと楽:

```powershell
wingetcreate submit packaging\winget
```

将来的にはリリースごとの更新自動化（[winget-releaser](https://github.com/vedantmgoyal9/winget-releaser)
等の GitHub Actions）を評価する（ADR 0045 決定 6）。

## scoop（`scoop/`）

> **保留中（[ADR 0048](../docs/v0.3/decisions/0048-scoop-defer-extras.md)）**: Extras への提出（[#18357](https://github.com/ScoopInstaller/Extras/pull/18357)）は知名度基準（星 100／fork 50 目安）未達でクローズされた。マニフェスト `winremap.json` は再申請用に残す。基準到達後に、まず Extras の package request を出してから URL・ハッシュを更新して再提出する。

- マニフェスト: `winremap.json`（portable `winremap.exe` を対象、`autoupdate` 定義つき）
- 提出先: 公式 [ScoopInstaller/Extras](https://github.com/ScoopInstaller/Extras) バケットへ PR（再開時）

ローカル検証:

```powershell
scoop install packaging\scoop\winremap.json   # ローカルファイルから直接テスト
```

## 参照（公式）

- winget マニフェスト仕様: https://learn.microsoft.com/en-us/windows/package-manager/package/manifest
- winget パッケージ提出: https://learn.microsoft.com/en-us/windows/package-manager/package/
- Scoop App Manifests: https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests
- Scoop Extras: https://github.com/ScoopInstaller/Extras
