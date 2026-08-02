# 公開運用・リリース手順（オーナー向け）

> ブリーフ §10（公開運用とリリースの完全性）を実際の操作手順に落としたもの。GitHub の Web UI での設定作業はオーナーのみが行う。

- 作成日: 2026-07-18
- 作成: Claude Code（AI モデル: claude-fable-5）／実施: オーナー
- 更新: 2026-07-29（Claude Code / claude-opus-5[1m]）— **§4 Microsoft Store を追加**。v0.6.0 以降、**1 回のリリースで更新するチャネルは 2 つ**になった
- 更新: 2026-07-30（Claude Code / claude-opus-5[1m]）— v0.6.0 の Store 提出と winget 差し替えの実施記録を §3・§4.2 に追記
- 更新: 2026-07-31（Claude Code / claude-opus-5[1m]）— v0.6.0 の**認定通過**を §4.2 に記録。P-8 が通り、v0.6 の受け入れが閉じた

---

## 1. 初回のみ: GitHub リポジトリ設定（v0.1.0 公開前に必須）

### 1.1 ブランチ保護（ブリーフ §10-1）

GitHub → リポジトリ → **Settings → Rules → Rulesets → New ruleset**（または Branches → Branch protection rules）で `main` に対して作成:

1. Target branches: `main`（Default branch）
2. 有効化する項目:
   - ✅ **Require a pull request before merging**（main への直接 push を禁止）
   - ✅ **Require review from Code Owners**（CODEOWNERS の強制。`AGENTS.md` / `docs/` / `.github/` / `SECURITY.md` の変更にオーナーレビューが必須になる）
   - ✅ **Require status checks to pass**: `check`（CI ジョブ）を指定
   - ✅ **Block force pushes**
3. Bypass list: 必要なら自分（オーナー）を追加。エージェント用トークンには付与しない

> 注意: ブランチ保護を有効にすると、以後エージェントは main へ直接 push できなくなる。開発フローが「ブランチ + PR」に変わるため、有効化のタイミングは v0.1.0 直前を推奨（それまでの開発速度を優先）。

### 1.2 Private Vulnerability Reporting（ブリーフ §10-4）

**Settings → Advanced Security（または Code security）→ Private vulnerability reporting → Enable**。SECURITY.md が案内する報告窓口はこの機能。

### 1.3 Actions の権限確認

**Settings → Actions → General**:

- Workflow permissions は既定の Read のままでよい（release.yml はワークフロー内の `permissions:` ブロックで必要権限を明示的に付与している）
- 「Allow GitHub Actions to create and approve pull requests」は **OFF のまま**

## 2. リリース手順（毎回）

> ブランチ運用は [04_git-branching.md](04_git-branching.md) に従う。リリース作業は
> `develop` から `release/<version>` を切って行い、`main` と `develop` の両方へ
> **`--no-ff` で**マージする。`main` へ直接トピックブランチを入れない。

0. **リリースブランチ**: `git checkout -b release/0.3.0 develop`
1. **受け入れテスト**: そのバージョンのチェックリストを実施し、結果を記録・コミットする（マージ前にリリースブランチ上で行う）。

   **v0.5 以降は「そのバージョンの 1 枚」だけを見ればよい。** v0.1〜v0.3 の約 200 項目は v0.5 で「自動で通す／手で通す／もう通さない」に仕分けてあり、手で通すものは [v0.5 チェックリスト §4](./v0.5/03_acceptance-checklist.md) の**手動最小集合 H-1〜H-9**（30〜40 分）に集約されている。各バージョンのチェックリストはこれを継承したうえで固有項目を足す形で作る。

   - 最新: [v0.6 受け入れチェックリスト](./v0.6/03_acceptance-checklist.md)（H-1〜H-9 ＋ MSIX 固有の P-1〜P-7）
   - 自動側: `cargo test` と `tests\ui\run-vm-ui-test.ps1`（[05_ui-test-automation.md](./05_ui-test-automation.md)）
   - 過去分（仕分けの根拠として参照する）: [v0.1](./v0.1/03_acceptance-checklist.md)・[v0.2](./v0.2/03_acceptance-checklist.md)・[v0.3](./v0.3/03_acceptance-checklist.md)・[v0.4](./v0.4/03_acceptance-checklist.md)
2. **CHANGELOG**: `Unreleased` の内容を新バージョン見出し（例 `## [0.1.0] - 2026-07-XX`）に切り出す
3. **バージョン**: `Cargo.toml` の `version` が**リリースする番号になっているか確認**する。番号を上げるのは開発の開始時であって、ここではない（[04_git-branching.md](04_git-branching.md) §2.6）。上がっていなければこの時点で上げる
4. **マージとタグ push**:

   ```powershell
   git checkout main
   git merge --no-ff release/0.3.0
   git tag -a v0.3.0 -m "WinRemap v0.3.0"
   git push origin main v0.3.0
   # リリース中の修正を開発側へ戻す
   git checkout develop
   git merge --no-ff release/0.3.0
   git push origin develop
   git branch -d release/0.3.0
   ```

5. release.yml が起動し、テスト → ビルド → インストーラー生成（Inno Setup、ADR 0027） → `SHA256SUMS` 生成 → **ビルド来歴の attestation** → **ドラフトリリース**作成まで自動で行う。**本文（リリースノート）は空で作られる**
6. **ドラフトの本文を入れる**（オーナー指示 2026-07-26。公開ボタンだけを押せば済む状態にして引き渡す）:

   ```powershell
   gh release edit v0.4.0 --notes-file <リリースノート.md>
   ```

   体裁は既存のリリースに合わせる（[v0.3.0](https://github.com/DaikiSuganuma/winremap/releases/tag/v0.3.0) が見本）:

   - **日本語 → `---` → 英語**の順で同じ内容を 2 本
   - 見出しは `## WinRemap vX.Y.Z — <その版を一言で>`、続けて 1 行の要約
   - `### 新機能` / `### New`（利用者から見た変化のみ。開発基盤の変更は書かない）、必要なら `### 修正` / `### Fixed`
   - `### インストール` / `### Install`、`### ダウンロードの検証` / `### Verify your download`（ハッシュ照合と `gh attestation verify` の 2 行。SmartScreen の断りも含めて既存の文面を流用）
   - 末尾に `**Full changelog:** https://github.com/DaikiSuganuma/winremap/blob/vX.Y.Z/CHANGELOG.md`

7. GitHub → Releases のドラフトを開き、以下を確認して **Publish release**（オーナーが行う）:
   - 添付物が `winremap.exe`・`winremap-setup.exe`・`SHA256SUMS`・`THIRD-PARTY-NOTICES.md` の 4 点であること（notices は exe 単体で落とす利用者向け。Bootstrap Icons の MIT 表示）
   - 本文の内容
8. 公開後の検証（利用者と同じ手順で最終確認）:

   ```powershell
   gh attestation verify .\winremap.exe --repo DaikiSuganuma/winremap
   gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap
   ```

9. **配布チャネルを更新する。リリース 1 回につき 2 つある**（v0.6.0 以降）:
   - **§4 Microsoft Store** — `.msix` を作って Partner Center へ提出する。ここが遅れると、**Store の利用者だけ古い版のまま**になる
   - **§3 winget** — マニフェストを新しいタグの URL と SHA256 に更新して提出する

   どちらも Release の公開後にしか行えない（§3 は資産の URL とハッシュが、§4 は Pages のプライバシーポリシー URL が、公開して初めて確定するため）。

## 3. パッケージマネージャーの更新（リリース後）

winget / scoop のマニフェストは公式 Releases の資産（URL と SHA256）を指すため、**タグを打って Release を公開したあとに**更新・提出する（[ADR 0045](./v0.3/decisions/0045-package-manager-channels.md)）。提出物の control copy は [`packaging/`](../packaging/) にある（書き方・ローカル検証は [`packaging/README.md`](../packaging/README.md)）。

0. **提出前チェック**: リリースする `winremap.exe` が OS 同梱以外の DLL に依存していないことを確認する。依存があるとインストールは通っても**起動時に `STATUS_DLL_NOT_FOUND` で落ち**、winget の検証で弾かれる（v0.3.0 で実際に発生。[作業ノート](./v0.4/notes/20260723_winget-0.3.0-validation.md)）。CRT は静的リンク済みなので（[ADR 0052](./v0.4/decisions/0052-static-crt.md)）、下記の出力が空であればよい:

   ```powershell
   $t = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes('.\winremap.exe'))
   [regex]::Matches($t, '(?i)(vcruntime|msvcp|api-ms-win-crt)[A-Za-z0-9_\-\.]*\.dll') | ForEach-Object Value | Sort-Object -Unique
   ```

1. `packaging/winget/*.yaml` と `packaging/scoop/winremap.json` の `PackageVersion` / `version`・`InstallerUrl` / `url`・SHA256 を新バージョンに更新する（ハッシュは Release の `SHA256SUMS` から。**winget は大文字**、scoop は小文字）
2. **winget**: `manifests/d/DaikiSuganuma/WinRemap/<version>/` として [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) へ PR（`wingetcreate submit packaging\winget` が楽。事前に `winget validate --manifest packaging\winget`）
3. **scoop**: **保留中**（[ADR 0048](./v0.3/decisions/0048-scoop-defer-extras.md)）。Extras は知名度基準未達でクローズ済み。基準到達後に再開する。`winremap.json` は `packaging/scoop/` に温存
4. `packaging/` の control copy を提出内容と同一にそろえてコミットする
5. 更新自動化（[winget-releaser](https://github.com/vedantmgoyal9/winget-releaser) 等）を入れるかは [ADR 0045](./v0.3/decisions/0045-package-manager-channels.md) 決定 6 に従って判断する

> 初回提出のみ審査に時間がかかる。README / ヘルプサイトの「パッケージマネージャーから入れる」記述は、マニフェストがマージされて初めて実際に解決するようになる。

> **2026-08-02 実施（v0.7.0）**: [PR #407875](https://github.com/microsoft/winget-pkgs/pull/407875) は**依然として OPEN**（初回登録の審査待ちが 3 版にまたがっている）。0.5.0・0.6.0 と**同じ判断を 3 度目**に繰り返し、新規 PR を出さずに中身を 0.7.0 へ差し替えた。**初回登録されるバージョンは 0.7.0 になる。** 差分はディレクトリのリネームと **7 行**（`PackageVersion` ×3・`InstallerUrl`・`InstallerSha256`・`ReleaseNotesUrl`・`ReleaseDate`）。`ProductCode` は Inno の `AppId` を変えていないので据え置き。提出前に **公開後の URL から `winremap-setup.exe` を実際に落として SHA256 が manifest と `SHA256SUMS` の両方に一致すること**を確認し、`winget validate` は提出物と `packaging/` の control copy の両方で通した。0. の DLL チェックも出力が空であることを確認済み。PR のタイトルを 0.7.0 に直し、経緯をコメントに残した。
>
> **次に同じ作業をするときの注意（2026-08-02 に踏んだ罠）**: 提出済みマニフェストは **CRLF** である。**Git Bash の `sed -i` を使わないこと** — 改行が LF に変換され、6 行のはずの差分が全行差分になる。PowerShell で `[IO.File]::ReadAllText` → `.Replace()` → `[IO.File]::WriteAllText`（`UTF8Encoding($false)`）と書けば CRLF が保たれる。fork のクローンは HTTPS だと非対話環境で push できないので、`git remote set-url origin git@github.com:...` に直す。
>
> **winget 初回登録の状況（2026-07-26 更新）**: v0.3.0 の提出（[PR #405731](https://github.com/microsoft/winget-pkgs/pull/405731)）は上記 0. の依存が原因で検証に失敗した。リリース済みバイナリは差し替えられないため、この PR は取り下げ、初回登録は v0.4.0 でやり直すことにした（オーナー決定 2026-07-23。経緯は[作業ノート](./v0.4/notes/20260723_winget-0.3.0-validation.md)）。
>
> **2026-07-30 実施**: [PR #407875](https://github.com/microsoft/winget-pkgs/pull/407875) は v0.6.0 公開時点でまだ OPEN（モデレーター承認待ち）だったため、v0.5.0 と**同じ判断を繰り返し**、新規 PR を出さずに中身を 0.6.0 に差し替えた。**初回登録されるバージョンは 0.6.0 になる。** 差分はディレクトリのリネームと 6 行のみ（`PackageVersion` ×3・`InstallerUrl`・`InstallerSha256`・`ReleaseNotesUrl`）。`ReleaseDate` は 0.5.0 の提出日と v0.6.0 の公開日が同じ 2026-07-29 なので変更なし。`ProductCode` も Inno の `AppId` を変えていないので据え置き。提出前に `winremap-setup.exe` を **URL から実際に落として SHA256 が manifest と `SHA256SUMS` の両方に一致すること**を確認し、`winget validate` は提出物と `packaging/` の control copy の両方で通した。PR のタイトルも 0.6.0 に直し、経緯をコメントに残した。
>
>
> **2026-07-29 実施（オーナー決定）**: v0.5.0 の公開時点で [PR #407875](https://github.com/microsoft/winget-pkgs/pull/407875) は未マージだったため、**新規 PR を出さず、#407875 の中身を 0.5.0 に差し替えた**。初回登録の PR が審査中に 2 本目の「新規パッケージ」PR を出すと重複扱いになり、どちらの審査も進まなくなるためである。**登録される最初のバージョンが 0.5.0 になる。**
>
> やり方（次に同じ状況になったとき用）: fork を **sparse checkout**（`git clone --filter=blob:none --sparse` ＋ `git sparse-checkout set manifests/d/<Publisher>`）する。winget-pkgs は 60 万ファイル超あり、素の clone は Windows のパス長制限で途中まで展開して失敗する。**マニフェストは PR 側のファイルを土台に、変更行だけ差し替える** — 提出済みのものは `wingetcreate` のヘッダー行と CRLF を持っており、`packaging/` の control copy をそのまま上書きすると全行差分になる。差し替えたら `winget validate <ディレクトリ>`、PR のタイトルも版に合わせ、何をしたかをコメントに残す。
>
> 提出前チェック（上記 0.）は実施済み: 公開した `winremap.exe` に `vcruntime` / `msvcp` / `api-ms-win-crt` の参照なし。`SHA256SUMS` と実ファイルのハッシュ一致、`gh attestation verify` も通ることを確認済み。
>
> **2026-07-26 実施**: v0.4.0 で再提出した（[PR #407875](https://github.com/microsoft/winget-pkgs/pull/407875)）。提出前に 0. のチェック（出力が空）、`winget validate`、公開済み `winremap-setup.exe` を実際に落として SHA256 が manifest と一致することを確認済み。あわせて #405731 を理由を書いて close した。**初回登録は審査に時間がかかる**ため、マージされるまで `winget install DaikiSuganuma.WinRemap` は解決しない。

## 4. Microsoft Store の更新（リリース後）

v0.6.0 で追加したチャネル（[ADR 0060](./v0.6/decisions/0060-msix-package.md)）。§3 の winget と違い、**参照ではなく実体を提出する**。Store は受け取った `.msix` を**再署名して配る**ため、GitHub Releases のバイナリとはファイルとして別物になる（中身は同じソース・同じタグから作った同じ exe である）。

Partner Center の製品は登録済みで、**これらの値は変えてはならない**。変えるとパッケージ ID が変わり、既存の利用者にとって別アプリになる。

| 項目 | 値 |
|---|---|
| Store ID | `9N6TQDXRX5WV` |
| Identity/Name | `SUGANUMADaiki.WinRemap` |
| Publisher | `CN=38CDEE8D-0FAC-4CBA-A3DA-17BBDD107F55` |
| PublisherDisplayName | `SUGANUMA Daiki` |
| パッケージファミリー名 | `SUGANUMADaiki.WinRemap_pktmgf1zdhxe0` |

### 4.1 順序制約（重要）

**Store 提出はタグを打って Release を公開したあと**である。理由は winget と違う。掲載情報に**プライバシーポリシーの URL** が要り、それを配信する GitHub Pages は `main` からのみ発行されるためである（`.github/workflows/pages.yml`）。

`site/` だけを `develop` 経由で先に `main` へ入れれば URL は生きる（[04_git-branching.md §2.5.1](./04_git-branching.md)）が、**それは抜け道として使わない**。提出するパッケージのバージョンに対応する Release がまだ無い状態でストアの審査が通ると、ストアからしか入手できない版が生まれ、SECURITY.md が読者に約束した「2 つの経路は同じソース・同じタグ」が崩れる。

提出前に **`https://daikisuganuma.github.io/winremap/privacy.html` が 200 を返すこと**を確認する。

### 4.2 手順

1. **パッケージを作る**:

   ```powershell
   .\packaging\msix\build.ps1 -Pack   # -> packaging\msix\out\winremap-<version>.msix
   ```

   **署名しない。** Store が再署名するので、こちらの署名は不要であり、そもそも Partner Center 発行の Publisher ID で署名できる証明書は買えない。`-SelfSign` は手元でインストール経路を試すためだけのもので、**提出には使わない**（自己署名の Publisher はマニフェストごと書き換わる）。

2. **バージョンは手で触らない。** `build.ps1` が `Cargo.toml` の `version` を読んで `AppxManifest.xml` の `Identity/@Version` に埋める。**第 4 フィールドは Store が使うので常に 0** である（`0.6.0` → `0.6.0.0`）。実行時に埋めた値が表示されるので、リリースする番号と一致しているか目で確認する。

3. **Partner Center へ提出する** — 製品 → 新しい申請 → パッケージに `.msix` をアップロードし、以下を確認する:
   - パッケージのバージョンと、公開しようとしている GitHub Release のタグが一致していること
   - 掲載情報（説明・機能・スクリーンショット）が入っていること。文面と画像は [Store 掲載情報の草案](./v0.6/notes/20260729_store-listing.md) にある
   - **`runFullTrust` の使用理由**を求められたら同ノート §3 の文面を貼る。低レベルキーボードフックはアプリコンテナでは動かない、という技術的必然性と、それ以外には使わないことの 2 点を答えるもの

4. **認定を待つ**（数時間〜数日）。通過するとストアページが公開される。

5. **公開後に確認する**（本バージョンの目的そのもの）:
   - `https://apps.microsoft.com/detail/9N6TQDXRX5WV` が開くこと
   - **Store からインストールしたときに SmartScreen の警告が出ないこと**
   - 「認定通過後に公開されます」の但し書きを文書から消す（**6 か所**。[v0.6 開発計画 §4.3](./v0.6/01_development-plan.md) に一覧）

> **2026-07-30 実施（v0.6.0）**: オーナーが `packaging\msix\out\winremap-0.6.0.msix`（4.3 MB・未署名・バージョン `0.6.0.0`）を Partner Center へ提出した。提出前に 4.1 の順序制約は満たしている（`privacy.html` が日英とも 200）。
>
> **2026-07-31 認定通過**: ストアページが公開され、`https://apps.microsoft.com/detail/9N6TQDXRX5WV` が 200 を返すようになった。オーナーが**ストアから実際にインストールし、SmartScreen の警告が出ないことを確認した**（[受け入れ](./v0.6/03_acceptance-checklist.md) **P-8**）。**これで v0.6 の受け入れが閉じた。** 手順 5 の但し書き削除（6 か所）も実施済み。**提出から公開まで 1 日**だった。
>
> 4.3 に「未確認」として残していた**落ちた申請を同じバージョン番号で出し直せるか**は、今回落ちなかったので依然として未確認である。

### 4.3 認定に落ちた場合

Partner Center の申請ページに落ちた項目と理由が出る。**落ちた申請は公開されていない**ので、直してその申請を差し替えればよい。GitHub Releases のように「出してしまったものは差し替えられない」制約は無い。

- **`.msix` の作り直しで済む場合**（マニフェストの不備など）: `build.ps1 -Pack` からやり直して同じ申請に上げ直す
- **exe の修正が要る場合**: `hotfix/*` を切って GitHub 側もリリースし直す。**Store だけ直して GitHub と中身がずれる状態を作らない**。SECURITY.md が「2 つの経路は同じソース・同じタグから作られる」と読者に約束しているためである

> **バージョン番号の再利用**: 公開済みバージョン以下の番号は提出できない。落ちた申請は公開されていないため同じ番号で出し直せる**はず**だが、**未確認である**。v0.6.0 の提出で実際に落ちた場合は、そこで分かったことをここに追記する。

### 4.4 このチャネルで注意すること

- **設定ファイルの場所が違う。** パッケージ版は `%APPDATA%` への書き込みがパッケージ専用フォルダーへリダイレクトされる（[ADR 0061](./v0.6/decisions/0061-packaged-config-path.md)）。利用者向けの説明は README・ヘルプサイト・FAQ に入れてある。**問い合わせを受けたときに最初に疑う差分がこれ**である
- **自動更新される。** GitHub 版と違い、利用者は明示的に更新しない。壊れた版を出すと全 Store 利用者に自動で届く。受け入れテスト（[v0.6 チェックリスト](./v0.6/03_acceptance-checklist.md)）を**リリースビルドで**通してから提出する
- **アンインストールの挙動が違う。** パッケージ専用フォルダーは消えるが、`%APPDATA%\winremap` に置いた設定（インストーラー版から移行した利用者のもの）は残る

## 5. 配布ポリシー（ブリーフ §10-3）

- **公式の配布経路は 2 つ**である。**Microsoft Store**（§4。Microsoft が再署名。SmartScreen の警告が出ない唯一の経路）と **GitHub Releases**（未署名。`SHA256SUMS` とビルド来歴で検証する）。どちらも同じソース・同じタグから作る
- winget（`DaikiSuganuma.WinRemap`）と scoop（公式 Extras バケット）は [ADR 0045](./v0.3/decisions/0045-package-manager-channels.md) で採用済みで、いずれも**公式 Releases の URL と SHA256 を参照する別の入口**である。実体を配っているわけではないので、上の「2 つ」には数えない
- 他サイトで配布されているバイナリは非公式（README / SECURITY.md に明記済み）
