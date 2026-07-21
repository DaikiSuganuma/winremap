# ADR 0045: winget / scoop の配布チャネル方針

- ステータス: 承認（オーナー指示）
- 日付: 2026-07-21
- 作成: Claude Code（AI モデル: claude-opus-4-8）

## 文脈

v0.1・v0.2 の配布は GitHub Releases のみで、インストーラー（Inno Setup 製）とポータブル exe、`SHA256SUMS`、ビルド来歴（attestation）を置いている。v0.2 の開発計画 Phase C で挙げ、v0.3 へ繰り越した「パッケージマネージャー対応」を実施する（[v0.3 開発計画 §3](../01_development-plan.md)）。

決めるべきことは 3 つ。**winget のパッケージ ID**、**scoop の提出先**、そして**この 2 つを公式チャネルと呼べるか**である。

## 決定

1. **winget のパッケージ ID は `DaikiSuganuma.WinRemap`** とする。短い名前で入れられるよう `Moniker: winremap` を併記し、`winget install winremap` でも導入できるようにする

2. **scoop は公式 Extras バケットへ提出する**（オーナー決定 2026-07-21）。自前バケットは作らない

3. **どちらも「公式チャネル」と位置づける。** マニフェストが参照するのは GitHub Releases の URL と、そこに置いた成果物の SHA256 だからである。README とヘルプサイトの「他サイト配布のバイナリは非公式」という記述は維持し、winget / scoop はその例外ではなく**公式 Releases への別の入口**であると書く

4. **提出は v0.3.0 のリリース後**に行う。マニフェストにはリリース資産の URL と SHA256 を書くため、タグを打つまで書けない

5. **インストーラー側の設定**: `InstallerType: inno`（Inno Setup 製。winget が silent 実行の引数を自動で補う）。scoop はポータブル exe を対象とし、`autoupdate` を定義する

6. **更新の自動化は winget から先に評価する。** リリースごとに手で出すのは続かない。[winget-releaser](https://github.com/vedantmgoyal9/winget-releaser) 等の GitHub Actions をリリースワークフローに足すかは、初回提出を手で通してから判断する

## 理由

- **`Publisher.Package` 形式は公式に定められている**（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest)）が、**個人開発者の Publisher 名についての規定は無い**。実例では kanata が `jtroo.kanata_gui` として登録されており、**GitHub アカウント名を Publisher に使う**のが個人開発者の通例である。`DaikiSuganuma` はリポジトリ URL と一致し、他の何とも衝突しない
- **Package 側を `WinRemap` と綴る**のは、表記規約（[ADR 0025](../../v0.1/decisions/0025-display-name-winremap.md)）が「文章・UI では WinRemap」としているため。winget のディレクトリは大文字小文字を区別し、`Microsoft.WindowsTerminal` など既存パッケージもパスカルケースである
- **短い名前は `Moniker` で与えるのが正しい仕組み**である。ID 自体を `winremap` のような一語にすると Publisher 部分が消えて形式から外れる。Moniker なら候補が一意である限り `winget install winremap` が通り、曖昧なときは完全な ID を案内するという winget 本来の挙動になる
- **Extras バケットを選ぶ理由**（オーナー決定）: 自前バケットは利用者に `scoop bucket add` を一手間強いる。Extras は既定でインストールされてはいないが広く知られており、「怪しい野良バケットを追加させる」形を避けられる。審査を通す手間は初回だけである
- **本体コードへの変更は不要**である。マニフェストは既存の配布物を指すだけで、アプリがネットワークに触れることはない（不変条件「ネットワーク通信を行うコードの追加禁止」に抵触しない）

## 却下した代替案

- **自前の scoop バケット（`suganuma/scoop-winremap`）**: 審査待ちが無く、リリース直後に反映できる。しかし利用者に追加コマンドを踏ませるうえ、「公式サイト以外から入れている」という感触を与える。維持するリポジトリも 1 つ増える。オーナー決定により Extras を選ぶ

- **パッケージ ID を `WinRemap.WinRemap` にする**: AutoHotkey（`AutoHotkey.AutoHotkey`）の形。組織や製品ブランドが実在する場合の書き方であり、個人プロジェクトで名乗ると実体のない発行元を作ることになる

- **パッケージ ID を `DKSG.WinRemap` 等の略称にする**: 短いが、GitHub アカウントともドメインとも一致せず、誰の発行物か検索で辿れない

- **Microsoft Store / MSIX での配布**: winget から一段と自然に入るようになるが、MSIX パッケージ化とストア審査、署名証明書の管理が増える。低レベルキーボードフックを使うアプリがストアの制約下で問題なく動くかの検証も別途必要になり、投じる労力に対して得るものが小さい

- **Chocolatey への提出**: 利用者層はあるが、winget が Windows 標準になった以降の新規プロジェクトで最初に置く場所ではない。要望が出てから検討する

- **リリース時に手作業で提出し続ける**: 初回はそれでよいが、リリースのたびに手順が増えると忘れる。自動化の評価を決定 6 として明示的に残す

## 参照（公式）

- winget パッケージ提出: https://learn.microsoft.com/en-us/windows/package-manager/package/
- マニフェスト仕様: https://learn.microsoft.com/en-us/windows/package-manager/package/manifest
- winget-pkgs リポジトリ: https://github.com/microsoft/winget-pkgs
- Scoop App Manifests: https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests
- Scoop Extras バケット: https://github.com/ScoopInstaller/Extras
