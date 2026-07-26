# 公開運用・リリース手順（オーナー向け）

> ブリーフ §10（公開運用とリリースの完全性）を実際の操作手順に落としたもの。GitHub の Web UI での設定作業はオーナーのみが行う。

- 作成日: 2026-07-18
- 作成: Claude Code（AI モデル: claude-fable-5）／実施: オーナー

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
1. **受け入れテスト**: 適用する全チェックリストを実施し、結果を記録・コミットする（マージ前にリリースブランチ上で行う）:
   - [v0.1 受け入れチェックリスト](./v0.1/03_acceptance-checklist.md) — リマップ基盤の回帰確認（全項目）
   - [v0.2 受け入れチェックリスト](./v0.2/03_acceptance-checklist.md) — ログ・設定ウィンドウ
   - [v0.3 受け入れチェックリスト](./v0.3/03_acceptance-checklist.md) — マクロ記憶。とくに **M-50 群**（`delay_ms = 15` × 20 コマンドの再生を、打鍵しながら 10 回繰り返してもフックが外れずリマップが生き続けること）は本リリースの最重要確認
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

5. release.yml が起動し、テスト → ビルド → インストーラー生成（Inno Setup、ADR 0027） → `SHA256SUMS` 生成 → **ビルド来歴の attestation** → **ドラフトリリース**作成まで自動で行う
6. GitHub → Releases のドラフトを開き、以下を確認して **Publish release**:
   - 添付物が `winremap.exe`・`winremap-setup.exe`・`SHA256SUMS`・`THIRD-PARTY-NOTICES.md` の 4 点であること（notices は exe 単体で落とす利用者向け。Bootstrap Icons の MIT 表示）
   - リリースノート（CHANGELOG から転記・調整）
7. 公開後の検証（利用者と同じ手順で最終確認）:

   ```powershell
   gh attestation verify .\winremap.exe --repo DaikiSuganuma/winremap
   gh attestation verify .\winremap-setup.exe --repo DaikiSuganuma/winremap
   ```

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

> **winget 初回登録の状況（2026-07-23 時点）**: v0.3.0 の提出（[PR #405731](https://github.com/microsoft/winget-pkgs/pull/405731)）は上記 0. の依存が原因で検証に失敗した。リリース済みバイナリは差し替えられないため、**この PR は取り下げ、初回登録は v0.4.0 でやり直す**（オーナー決定 2026-07-23。経緯と手順は[作業ノート](./v0.4/notes/20260723_winget-0.3.0-validation.md)）。

## 4. 配布ポリシー（ブリーフ §10-3）

- 配布の一次は GitHub Releases。winget（`DaikiSuganuma.WinRemap`）と scoop（公式 Extras バケット）は [ADR 0045](./v0.3/decisions/0045-package-manager-channels.md) で採用済みで、いずれも公式 Releases の URL と SHA256 を参照する**別の入口**である。**マニフェストの確定・提出は v0.3.0 のリリース後**（タグを打つまで資産の URL とハッシュが定まらないため）。パッケージ更新手順は Phase B で本書に追記する
- 他サイトで配布されているバイナリは非公式（README / SECURITY.md に明記済み）
