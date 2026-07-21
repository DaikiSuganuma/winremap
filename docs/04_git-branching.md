# Git ブランチ運用

> 元資料: Vincent Driessen, *A successful Git branching model* (2010, 2020 追記)
> — https://nvie.com/posts/a-successful-git-branching-model/
> （通称 git-flow。本書は原典の要約と、本プロジェクトへの適用を分けて記述する）

- 作成日: 2026-07-21
- 作成: Claude Code（AI モデル: claude-opus-4-8）／レビュー・承認: オーナー

---

## 1. 原典の要約（git-flow）

### 1.1 恒久ブランチ 2 本

| ブランチ | 意味 |
|---|---|
| `master` | **本番の状態**。ここへのコミットは必ず 1 つのリリースに対応する |
| `develop` | **次のリリースに向けた統合先**。HEAD は常に「次に出すもの」の最新状態 |

### 1.2 支援ブランチ 3 種

いずれも一時的で、役目を終えたら削除する。

| 種類 | 分岐元 | マージ先 | 命名 |
|---|---|---|---|
| feature | `develop` | `develop` | 任意（`master` / `develop` / `release-*` / `hotfix-*` 以外） |
| release | `develop` | `develop` と `master` | `release-*` |
| hotfix | `master` | `master` と `develop`（release 進行中ならそちら） | `hotfix-*` |

- **feature**: 次以降のリリースに入れる機能を作る。いつ完成するか分からないものはここに留める
- **release**: リリース準備専用。バージョン番号の更新、メタ情報の整備、最終的なバグ修正のみを行う。
  ここを切った時点で `develop` は「次の次」の受け入れを再開できる
- **hotfix**: 本番の緊急障害に対応する。`develop` の作業中の状態を巻き込まずに修正を出せる

### 1.3 `--no-ff` を必ず使う

原典が明示的に強調している点である。

> "always create a new commit object, even if the merge could be performed with a fast-forward"

fast-forward でマージすると、**そのブランチが存在したという情報が履歴から消える**。
一連のコミットが 1 つの機能をなしていたことが分からなくなり、機能単位で戻すことも難しくなる。
`--no-ff` は空のマージコミットを 1 つ増やすが、その代償は履歴の可読性に見合う。

```
$ git checkout develop
$ git merge --no-ff myfeature
$ git branch -d myfeature
```

### 1.4 原典の 2020 年の追記

著者自身が「万能薬は存在しない。自分の文脈で考えよ」と書き添えている。
git-flow は 2010 年当時、**明示的にバージョン管理され、複数バージョンを並行サポートする
ソフトウェア**を想定していた。継続的にデプロイする Web アプリなら GitHub Flow のような
単純な運用の方が適する、というのが著者の後年の見解である。

---

## 2. WinRemap への適用

### 2.1 なぜ git-flow 側を採るか

WinRemap は**明示的にバージョン管理されたデスクトップアプリ**である。
利用者はタグ付きリリースからバイナリを取得し、ビルド来歴（attestation）で検証する
（[03_release-operations.md](03_release-operations.md)）。継続デプロイではないため、
著者が「git-flow が今も適する」と言った側に当てはまる。

一方で、開発者はオーナー 1 名（＋ AI エージェント）である。原典をそのまま持ち込むと、
1 人しかいない統合作業のために恒久ブランチが 1 本増える。そこで**次のように適用する**。

### 2.2 採用するもの

| 原典 | 本プロジェクト | 備考 |
|---|---|---|
| `master` | **`main`** | 名称のみ変更（GitHub の既定に合わせる） |
| `develop` | **採用する** | 下記 2.3 |
| feature | **`feature/*`** | 機能追加 |
| release | **`release/*`** | リリース準備。CHANGELOG の切り出し、バージョン更新、受け入れテストの記録 |
| hotfix | **`hotfix/*`** | 公開済みバージョンの緊急修正 |
| `--no-ff` | **必須** | 例外なし。§2.5 |

feature 以外に、機能追加でない作業のための種別を足す（原典の feature の位置づけと同じ扱い）。

- `fix/*` — 未リリースの不具合修正
- `docs/*` — ドキュメントのみの変更
- `chore/*` — 依存更新、CI 設定など

### 2.3 `develop` を置く理由

置かない場合、`main` には開発途中のコミットが並ぶ。実際 v0.2.0 の開発では、
`main` が 37 コミット分だけ「どのリリースにも含まれない状態」を通過した。
**`main` を clone した人が、公開したことのないコードを手にする**ことになる。

`develop` を置けば `main` は常に「公開済みの状態」となり、次が成り立つ。

- タグと `main` の HEAD が原則一致し、hotfix の分岐元が自明になる
- 「公式バイナリは Releases のみ」という配布方針（ブリーフ §10-3）とソースの状態が一致する

コストは、リリース時に `release/*` を `main` と `develop` の両方へマージする手間である。
リリースは年に数回であり、割に合うと判断した。

### 2.4 ブランチの流れ

```
feature/xxx ──┐
fix/xxx ──────┼──▶ develop ──▶ release/0.3.0 ──┬──▶ main（タグ v0.3.0）
docs/xxx ─────┘         ▲                      │
                        └──────────────────────┘（release を develop にも戻す）

hotfix/0.3.1 ◀── main ────────────────────────┬──▶ main（タグ v0.3.1）
                                              └──▶ develop
```

### 2.5 マージの規則

**すべてのマージで `--no-ff` を指定する。** fast-forward させない。

```powershell
git checkout develop
git merge --no-ff feature/config-editor
git branch -d feature/config-editor
git push origin --delete feature/config-editor
```

- マージコミットのメッセージには**そのブランチが何をしたのか**を書く。既定の
  "Merge branch 'x'" のままにしない
- `main` へのマージは `release/*` と `hotfix/*` のみ。トピックブランチを直接入れない
- マージ済みブランチはローカル・リモートとも削除する（オーナー決定。残すと
  一覧が読めなくなる）

> **v0.2.0 の実績**: `v0.2-plan` を `--ff-only` で `main` へ入れた（2026-07-21）。
> 本ルールの制定はその直後であり、公開済みの履歴は書き換えない。v0.3.0 以降に適用する。

### 2.6 ブランチ保護との関係

`main` は PR 必須・CI 必須で保護する（[03_release-operations.md](03_release-operations.md) §1.1）。
GitHub の PR で "Create a merge commit" を選べば `--no-ff` 相当になる。
**"Squash and merge" と "Rebase and merge" は使わない** — どちらもブランチが存在した
情報を消すため、§2.5 の目的に反する。

### 2.7 やらないこと

- **`develop` へのブランチ保護**: オーナー 1 名の統合先であり、PR を必須にする相手がいない
- **git-flow ツール（`git flow` コマンド）の導入**: 覚えることが増える割に、
  上記のコマンド列は素の git で足りる
- **複数バージョンの並行サポート**: 常に最新版のみをサポートする。原典の
  「複数バージョンを支える」側面は本プロジェクトには不要である
