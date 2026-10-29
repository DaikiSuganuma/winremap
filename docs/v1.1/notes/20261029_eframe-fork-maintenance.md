# eframe のフォークを egui の新しい版に載せ替える手順

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- 根拠: [ADR 0088](../decisions/0088-keep-the-eframe-fork.md)（フォークを持ち続ける）、[ADR 0055](../../v0.5/decisions/0055-accesskit-for-child-viewports.md)（何を変えているか）、[ADR 0074](../../v0.9/decisions/0074-egui-0-36-and-the-upstream-pr.md)（0.35 → 0.36 のときに踏んだこと）

## 何が入っているか

フォーク `https://github.com/DaikiSuganuma/egui` のブランチ `accesskit-child-viewports-<minor>` は、**egui の該当タグ ＋ 1 つの変更**である。変更は「eframe が子ビューポート（`show_viewport_deferred` で作る窓）にも AccessKit のアダプターを作る」で、`crates/eframe/src/native/glow_integration.rs` と `wgpu_integration.rs` の 2 ファイルに閉じる（ADR 0074。WinRemap が使うのは glow 側だが、両方に入れてある）。

v1.1 時点:

| 項目 | 値 |
|---|---|
| ブランチ | `accesskit-child-viewports-0.36` |
| 元のタグ | `0.36.1` |
| `Cargo.lock` が指すコミット | `2ba50bdb` |
| `Cargo.toml` | `eframe = "0.36.1"`（`[dependencies]`）、`winit = "0.30.13"`、`[patch.crates-io] eframe = { git = …, branch = "accesskit-child-viewports-0.36" }` |

## 載せ替えの手順（egui の minor が上がったとき）

1. **新しいブランチを切る。** フォークで `git checkout -b accesskit-child-viewports-<新 minor> <新タグ>`（例: `0.37.0`）。古いブランチは消さない — 古い版の WinRemap をビルドし直せるようにしておく
2. **変更を載せる。** 前のブランチの差分コミット（`git log <元タグ>..accesskit-child-viewports-<前 minor>` で 1 つ）を `git cherry-pick` する。衝突したら、2 ファイルの該当箇所（ルートビューポートに AccessKit を付けている場所の隣）を読んで、同じことを子ビューポートの生成経路にも行う形で直す
3. **push する。** `git push origin accesskit-child-viewports-<新 minor>`
4. **WinRemap 側の `Cargo.toml` を直す。** `eframe` の版、`[patch.crates-io]` の `branch`、そして **`winit` の版を eframe が使うものに揃える**（`cargo tree -p eframe -i winit` で見る。違うと `gui/mod.rs` の `with_any_thread` がコンパイルエラーになる — それが揃っていない印）
5. `cargo update -p eframe -p egui -p winit` で `Cargo.lock` を更新し、`cargo fmt` / `cargo clippy --all-targets -- -D warnings`（`--features test-inject` も）/ `cargo test`
6. **UI テストで確かめる。** `tests/ui/run-vm-ui-test.ps1` の `01-settings-window` と `04-log-window` が通ること — 子ビューポートの UIA ツリーに部品が見えるかが、この変更の唯一の効き目である（[05_ui-test-automation.md](../../05_ui-test-automation.md) §3）
7. **受け入れの H-3** を次の版で省略しない（egui の版が上がると見た目の土台が変わる。v0.9 と同じ）
8. 変更内容を ADR 0088 の追記か、新しい ADR（API が変わって cherry-pick で済まなかった場合）に書く

## 外すとき

egui のリリースノートに子ビューポートの AccessKit 対応が載ったら: `[patch.crates-io]` を消し、`eframe` をその版にして手順 5〜7 を行い、ADR を書く。
