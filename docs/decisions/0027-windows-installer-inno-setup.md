# ADR 0027: Windows インストーラーを Inno Setup で追加（ユーザー単位・管理者権限不要）

- ステータス: 承認（オーナー指示）
- 日付: 2026-07-20
- 作成: Claude Code（AI モデル: claude-fable-5）
- 参照: [Inno Setup 公式サイト](https://jrsoftware.org/isinfo.php) / [Inno Setup ドキュメント](https://jrsoftware.org/ishelp/) / [GitHub Actions windows-latest プリインストール一覧](https://github.com/actions/runner-images/blob/main/images/windows/Windows2022-Readme.md)

## 文脈

v0.1.0 リリースにあたり、オーナーから「Windows のインストーラーを作成してほしい」との指示。これまでの配布物は単一 exe（`winremap.exe`）+ `SHA256SUMS` のみで、利用者は exe の置き場所・スタートアップ登録・初期設定ファイルの作成をすべて手作業で行う必要があった。トレイ常駐型ツールとしては「サインイン時に自動起動」まで含めた導入を 1 クリックで済ませたい。

配布ポリシー（ブリーフ §10-3: GitHub Releases のみ）は変更しない。インストーラーも同じ Releases に添付し、SHA256SUMS と build provenance attestation の対象に含める。

## 決定

**Inno Setup 6 のスクリプト（`installer/winremap.iss`）でインストーラー `winremap-setup.exe` を生成し、従来の単一 exe と併せて GitHub Releases に添付する。**

1. **ユーザー単位インストール**（`PrivilegesRequired=lowest`、既定先 `%LOCALAPPDATA%\Programs\WinRemap`）。UAC 昇格なしで導入できる
2. インストーラーが行うこと:
   - `winremap.exe`・LICENSE・README・`examples\*.toml` の配置
   - スタートメニューショートカット作成
   - 任意タスク（既定 ON）: スタートアップ登録（`shell:startup` へのショートカット。レジストリ Run キーは使わない）
   - `%APPDATA%\winremap\config.toml` が **存在しない場合のみ** `examples/minimal.toml` を初期設定として配置（既存設定は絶対に上書きしない。アンインストール時も削除しない）
3. 既存の単一インスタンス用 named mutex（`Local\winremap-single-instance`）を `AppMutex` に指定し、実行中のインストール/アンインストールで終了を促す
4. ビルドは release.yml に 1 ステップ追加（`iscc` は windows-latest ランナーにプリインストール済み）。バージョンはタグから `/DAppVersion=` で注入
5. 従来の単一 exe 配布（ポータブル利用）は継続する。インストーラーはあくまで追加の選択肢

## 理由

- **CI への追加依存ゼロ**: Inno Setup は GitHub Actions の windows ランナーにプリインストールされており、ダウンロードステップ（= 新たなサプライチェーン面）を増やさずに済む
- **スクリプトがリポジトリ内で完結**: `.iss` はテキスト 1 ファイルで、CODEOWNERS / レビューの統制下に置ける
- **ユーザー単位インストールが製品特性に合う**: フックはセッション単位で動き、管理者権限ウィンドウには昇格しても UIPI の制約が残る（README Limitations）。per-machine インストールに実利がなく、UAC なしの方が導入障壁が低い
- **日本語 UI が公式同梱**: Inno Setup は日本語翻訳（`Japanese.isl`)を公式に同梱しており、本体の日英 UI 方針（ADR 0014）と揃う
- **AppMutex が既存設計とそのまま噛み合う**: 単一インスタンス保証に使っている named mutex を書くだけで「実行中は終了を促す」動作になる
- ライセンス面: Inno Setup で生成したインストーラーの配布は無償・無制限（Inno Setup License）。生成物に LGPL 等のコードは混入しない

## 却下した代替案

- **WiX Toolset / cargo-wix（MSI）**: MSI はエンタープライズ配布（GPO 等）では有利だが、XML 定義が冗長で per-user インストールの設定が煩雑。個人向けツールに MSI の利点が薄く、「単純さ > 機能」の優先順位に反する
- **NSIS**: 実績はあるがスクリプト言語が独特で、モダンな per-user 対応や日本語 UI の扱いが Inno Setup より手数が多い
- **MSIX**: ストア形式。署名証明書が必須で、低レベルフック常駐アプリとの相性（AppContainer 制約）の検証コストが高い
- **winget 公開**: 配布チャネルの追加であり別判断（ブリーフ §10-3 どおり、必要になった時点で別 ADR）。なおインストーラー化により将来の winget 対応（installer type: inno）への布石にはなる
- **ポータブル exe のみ継続（現状維持）**: スタートアップ登録や初期設定の手作業が残り、オーナーの指示（導入を簡単に）を満たさない
