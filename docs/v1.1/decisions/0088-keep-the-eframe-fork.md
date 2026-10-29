# ADR 0088: eframe のフォークは持ち続け、egui のリリースごとに載せ替える手順を文書にする

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- ステータス: **承認（[v1.1 開発計画](../01_development-plan.md) G2、オーナー決定 2026-10-29「(a) フォーク維持＋手順の文書化」）**。[ADR 0074](../../v0.9/decisions/0074-egui-0-36-and-the-upstream-pr.md) 決定 5 の「外す条件」を置き換える
- 関連: [ADR 0055](../../v0.5/decisions/0055-accesskit-for-child-viewports.md)（子ビューポートに AccessKit を付ける方式と `[patch.crates-io]` の暫定運用）、[ADR 0074](../../v0.9/decisions/0074-egui-0-36-and-the-upstream-pr.md)（0.36.1 への載せ替えと upstream PR、その追記: PR のクローズ）、[ADR 0064](../../v0.7/decisions/0064-winapp-cli-for-ui-tests.md)（UI テストは UI Automation で窓を読む）
- 手順: [notes/20261029_eframe-fork-maintenance.md](../notes/20261029_eframe-fork-maintenance.md)

## 背景

設定ウィンドウとログウィンドウは不可視ホストの子ビューポート（ADR 0037）で、eframe はルートにしか AccessKit を付けない。UI テスト（ADR 0064）は UI Automation で窓を読むので、子ビューポートにアダプターが要る。ADR 0055 はそれを eframe 本体へ出す PR を本筋とし、マージまでの間だけ `[patch.crates-io]` でフォークを指すと決めた。`Cargo.toml` のコメントも「**upstream に入ったら消す**」と言っている。

その前提は 2026-09-14 に消えた。PR [emilk/egui#8410](https://github.com/emilk/egui/pull/8410) は 1 か月レビューが付かず、`main` と競合し、オーナーがクローズした（ADR 0074 追記）。**「一時しのぎ」が一時しのぎでなくなったのに、文書はそう書いたまま**で、egui の次のリリースが出るたびに「このパッチをどうするか」を考え直すことになる。

## 決定

### 決定 1 — フォークを持ち続ける

`[patch.crates-io]` は残す。フォーク（`DaikiSuganuma/egui`）のブランチは `accesskit-child-viewports-<egui の minor>` と名付け、egui の minor が上がるたびに**新しいブランチ**を切って載せ替える。v1.1 時点のブランチは `accesskit-child-viewports-0.36`（0.36.1 ＋ 1 変更、`Cargo.lock` のコミット `2ba50bdb`）。

### 決定 2 — 載せ替えの手順を 1 枚のノートにする

[notes/20261029_eframe-fork-maintenance.md](../notes/20261029_eframe-fork-maintenance.md)。「どのブランチから、何を、どこへ、何で確かめるか」。`Cargo.toml` のコメントはこのノートを指し、「消す」とは言わない。

### 決定 3 — 外す条件は「上流に同等の機能が入ったら」

PR を出し直す予定は置かない。egui のリリースノートに子ビューポートの AccessKit 対応が載ったら、そのときに ADR を書いて外す。確かめ方は UI テストの `01-settings-window`（設定ウィンドウの UIA ツリーに部品が見えるか）。

### 決定 4 — `winit` の直接依存は eframe の版に揃える

`winit` は `gui/mod.rs` の `EventLoopBuilderExtWindows::with_any_thread` 1 つのためだけに直接依存している。eframe が使う `winit` と版が違うとトレイトが eframe のビルダーに当たらず、コンパイルエラーで分かる。載せ替えの手順にこの更新を含め、`Cargo.toml` にも 1 行書く（開発計画 G3）。

## 理由

- **決めないことの費用が毎リリースかかる。** 0.35 で 2 版、0.36 で 3 版、同じ判断をしている
- **UI テストはこの機能に依存している。** 外せば `tests/ui/` の設定ウィンドウとログウィンドウの検査が動かなくなる。代替方式（画面キャプチャの比較、`winapp` の座標クリック）はどれも脆い
- **手順が文書にあれば、載せ替えは作業であって判断ではない**

## 却下した代替案

- **PR を出し直す** — 1 か月反応が無かった PR をそのまま出し直しても結果は変わらない。出し直すなら egui 側の議論（issue）から始める必要があり、それはこの版の仕事ではない
- **子ビューポートの AccessKit を諦め、UI テストを別方式にする** — 開発計画 G2 の (b)。UI テストの作り直しは振る舞いを変えない版の範囲を超える
- **フォークをやめて、`eframe` をベンダリング（`[patch]` でローカルパス）する** — 差分の所在が見えにくくなる。フォークはブランチ名とコミット 1 つで差分を言える

## 影響・補足

- `Cargo.toml` の `[patch.crates-io]` の注記を、現状の方針とノートへの参照に書き換える
- `docs/03_release-operations.md` は変えない（載せ替えはリリース作業ではなく、依存更新の作業）
