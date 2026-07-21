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

1. **受け入れテスト**: [05_acceptance-checklist.md](./v0.1/03_acceptance-checklist.md) の「リリース前フルチェック」全項目を実施し、結果を記録・コミット
2. **CHANGELOG**: `Unreleased` の内容を新バージョン見出し（例 `## [0.1.0] - 2026-07-XX`）に切り出す
3. **バージョン**: `Cargo.toml` の `version` を更新（`Cargo.lock` も追随）
4. **タグ push**:

   ```powershell
   git tag v0.1.0
   git push origin v0.1.0
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

## 3. 配布ポリシー（ブリーフ §10-3）

- 配布は GitHub Releases のみ。crates.io / winget への展開は必要になった時点で ADR（Architecture Decision Record: 設計判断の記録）を書いて判断する
- 他サイトで配布されているバイナリは非公式（README / SECURITY.md に明記済み）
