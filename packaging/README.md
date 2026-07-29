# packaging/ — パッケージマネージャー用マニフェスト

- 作成日: 2026-07-22
- 作成: Claude Code（AI モデル: claude-opus-4-8）／レビュー・承認: オーナー

WinRemap を winget / scoop から入れられるようにするためのマニフェスト置き場。方針は
[ADR 0045](../docs/v0.3/decisions/0045-package-manager-channels.md) を参照。**いずれも
公式 GitHub Releases の資産（URL と SHA256）を指す**ため、他サイト配布とは別物の「公式
Releases への別の入口」である。

マニフェストは各バージョンのリリース後に、その資産の URL とハッシュを埋めて更新する
（[docs/03_release-operations.md](../docs/03_release-operations.md) §3）。ここに置くのは
提出物の control copy であり、実際に配信されるのは下記の各リポジトリ側のコピーである。

`msix/` だけは事情が違う。**参照ではなく配信物そのもの**を作って提出する。手順は
[docs/03_release-operations.md](../docs/03_release-operations.md) **§4**。

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

## Microsoft Store / MSIX（`msix/`）

winget・scoop と違い、**これだけは配信物そのものを作る**。Microsoft がパッケージを再署名するため、
Store 経由で入れた利用者には SmartScreen の「Windows によって PC が保護されました」が出ない。
方針と実測結果は [ADR 0060](../docs/v0.6/decisions/0060-msix-package.md) を参照。

- 予約済み製品: Store ID `9N6TQDXRX5WV`、パッケージ ID `SUGANUMADaiki.WinRemap`
- パッケージファミリー名: `SUGANUMADaiki.WinRemap_pktmgf1zdhxe0`
- 構成: `AppxManifest.xml`（Identity は Partner Center 発行値）、`Assets/`（SVG から生成、コミット済み）、`build.ps1`

```powershell
# 動作確認（Developer Mode が必要。証明書も管理者権限も不要）
packaging\msix\build.ps1 -Register

# Partner Center へ上げる .msix を作る（署名しない — Store が署名する）
packaging\msix\build.ps1 -Pack

# 署名インストールの経路まで試す（証明書の信頼に管理者権限が要る）
packaging\msix\build.ps1 -SelfSign
```

アイコンを描き直したときは、`assets/svg/kbd-enabled.svg` を差し替えて再生成する:

```powershell
cargo run --example msix_assets
```

掲載用スクリーンショットも同じフォルダーで作る。撮影には**専用の設定**
（`screenshot-demo.{en,ja}.toml`）を使う — `examples/` は手本であって画像の仕事とは別であり、
個人の設定は公開画像に名前が載るためである。

```powershell
# 掲載画像を撮る（日英各 4 枚。人がメモ帳を開いてから実行する）
packaging\msix\capture-screenshots.ps1

# 同じ画像の余白を切り詰めて README 用に書き出す -> site/assets/screenshots/
packaging\msix\export-doc-images.ps1
```

README 用を撮り直さないのは、**撮り直せば必ずいつか食い違う**ためである。Store 用は
Partner Center の最小解像度に合わせて 1920×1080 の無地キャンバスに合成してあり、
`export-doc-images.ps1` はそこから余白を落とすだけである。

`layout/`・`out/`・`screenshots/` は生成物で Git 管理外。ただし
`site/assets/screenshots/` へ書き出した README 用の 4 枚はコミットする。

## 参照（公式）

- MSIX パッケージ（コマンドラインからの作成）: https://learn.microsoft.com/en-us/windows/msix/package/manual-packaging-root
- Store 提出時のパッケージ要件: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements
- winget マニフェスト仕様: https://learn.microsoft.com/en-us/windows/package-manager/package/manifest
- winget パッケージ提出: https://learn.microsoft.com/en-us/windows/package-manager/package/
- Scoop App Manifests: https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests
- Scoop Extras: https://github.com/ScoopInstaller/Extras
