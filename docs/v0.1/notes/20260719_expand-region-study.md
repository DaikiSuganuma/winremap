# 検討ノート: 選択範囲の段階的拡大（expand-region）の Windows 実装

- 作成日: 2026-07-19
- 作成: Claude Code（AI モデル: Fable 5 / claude-fable-5）
- 種別: 調査・検討ノート（実装は未着手、方向性も未決定）

## 1. 背景と論点

Emacs の expand-region（キーを押すたびに選択範囲を単語 → 行 → 段落 → 全体と段階的に拡大する機能）に相当するものが Windows にあるか、なければ WinRemap の組み込み機能として実装できるか、を調査した。

## 2. Windows ネイティブの提供状況

OS レベルでは提供されていない。Windows が提供するのは各アプリ共通の選択系ショートカット（`Ctrl+Shift+→` の単語単位拡張、ダブルクリックの単語選択、`Ctrl+A` の全選択など）まで。段階的な意味単位の拡大は VS Code（`Shift+Alt+→`）や Visual Studio（`Ctrl+W`）など個々のエディタが独自実装しているもので、テキスト選択の管理自体が各アプリ内部の仕事であるため OS 側には存在しない。

## 3. 実装アプローチ

### 3.1 UIA（UI Automation）の TextPattern を使う方法

Windows のアクセシビリティ API である UIA には `ITextRangeProvider::ExpandToEnclosingUnit` があり、選択範囲を Character → Word → Line → Paragraph → Document の単位で拡大できる。expand-region の思想にかなり近い。

- 長所: 意味単位での正確な拡大ができる
- 短所: 対象アプリのフォーカス中コントロールが TextPattern を実装している必要がある。構文（括弧対応・関数単位）は理解しない

### 3.2 キーストローク合成で近似する方法

`Ctrl+Shift+→` や `Home` → `Shift+End` → `Ctrl+A` のような選択系ショートカットをマクロ送出し、押下ごとに「単語 → 行 → 全体」と状態遷移させる。WinRemap の既存マクロ機構（SendInput ベース）の延長で作れる。

- 長所: アプリを選ばない、実装が軽い
- 短所: 近似にすぎない。縮小（shrink）が正確にできない。段落選択は汎用合成できないため「全体」に丸めるしかない

### 3.3 ハイブリッド方式（検討したプラン）

実行時にフォーカス中コントロールの TextPattern 対応を判定し、対応していれば UIA、非対応ならキーストローク合成にフォールバックする。TOML 設定で `"C-w" = "@expand-region"` のような `@` プレフィックスの組み込みアクション記法を導入する。

## 4. WinRemap への実装可能性 — 可能（設計要点）

- 現在のアクションは `Output::Chord`（単一コマンド）と `Output::Seq`（マクロ）の静的置換のみ。`Output::Builtin(BuiltinAction)` を追加する初の「組み込みアクション」カテゴリになる
- 段階カウンタは 2 ストローク sequence の `PENDING` と同じ thread_local パターンで保持。リセット条件は「他キー押下」「フォアグラウンド変更」「トレイ無効化」
- **最大の設計上の壁**: AGENTS.md 不変条件 2（フックコールバック内でのヒープ確保・重い Win32 呼び出し禁止）。UIA は COM（Component Object Model）のプロセス間呼び出しで数百 ms ブロックし得るため、専用ワーカースレッドに `PostThreadMessageW` で委譲する設計が必要（不変条件の例外リスト改訂 = オーナー承認事項）
- TextPattern 対応は exe 単位でなくフォーカス中コントロール単位で変わるため、アプリごとの判定キャッシュは作らず毎回判定する
- フェーズ分割: lib 層（記法・コンパイル）→ キーストローク合成のみの同期版（この時点で全アプリ動作）→ UIA ワーカー追加、の順で段階的に実装可能

詳細な実装ステップはプランファイル（`~/.claude/plans/1-uia-textpattern-splendid-hennessy.md`）に記録済み。

## 5. デメリット評価

1. **UIA 対応状況のばらつき（最大の弱点）**: VS Code など Electron 系エディタはスクリーンリーダー検知までアクセシビリティツリーを無効化しており、プログラマが最も使いたいアプリでフォールバック側に落ちがち。ターミナルも非対応
2. **レイテンシ**: プロセス間 COM 呼び出しのため 1 回の拡大に数十〜数百 ms かかる場合がある
3. **構文認識は不可能**: 拡大単位は word/line/paragraph/document のみ。括弧対応・関数単位など expand-region 本家の一番の強みは再現できない
4. **状態のズレ**: 段階カウンタは WinRemap 側が持つため、マウスクリックでの選択し直しを検知できず（キーボードフックはマウスを見ない）、実際の選択状態とズレ得る
5. **コードベースの複雑化**: 初の COM 利用・ワーカースレッド・不変条件の例外拡張により、「静的なキー置換だけ」というシンプルさを一段崩す。unsafe 面積も増える
6. **日本語との相性**: UIA の word 単位もフォールバックの `Ctrl+Shift+→` も、日本語の単語境界の扱いはアプリ依存で分節が不正確なことが多い

総合評価: 対応アプリの狭さを考えると UIA 経路の費用対効果はやや微妙。まずキーストローク合成のみの軽量版で日常運用し、物足りなければ UIA を足す判断も合理的。

## 6. 代替・追加の機能候補（費用対効果順）

1. **Emacs mark mode（おすすめ度: 高、UIA 不要）**: `C-Space` でマークを立て、以降の移動キーを自動的に `Shift+移動` に変換して選択を伸ばす。純粋なキーストローク変換なので全アプリで動作し、既存アーキテクチャとの相性も良い。xremap が `set_mark` として同等機能を持つ
2. **クリップボードを汚さない選択テキスト取得（UIA の真価が出る用途）**: TextPattern の `GetSelection → GetText` で `Ctrl+C` を送らずに選択テキストを読める。「選択テキストをブラウザで検索」「選択 URL を開く」等をクリップボード上書きなしで実現できる
3. **launch-or-activate（UIA 不要）**: アプリ起動 or 既存ウィンドウへフォーカス。AutoHotkey の定番用途
4. **smart-home**: 行頭とインデント後先頭のトグル。フォールバック合成だけでは正確に作れず UIA で精度が上がるタイプ
5. **paste-as-plaintext**: 書式なし貼り付けの統一

expand-region を実装する場合も、mark mode と同じ「組み込みアクション + 状態機械」基盤に相乗りさせると設計の元が取れる。

## 7. xremap との比較

xremap（Linux, evdev/uinput レベル）に同等機能はない。アプリ別リマップ・キーシーケンス送出・Emacs 風 mark mode（`set_mark`）は持つが、すべてキーストローク合成の範囲。Linux 側のアクセシビリティ API（AT-SPI）でテキスト構造を照会して意味単位で選択を拡大する機構は持たないため、UIA 経路を実装すれば WinRemap 独自の付加価値になる。

## 8. 結論・未決事項

- 実装は技術的に可能。ただし方向性（フル版 / 軽量版 / mark mode 優先 / 見送り）はオーナー判断待ち
- どの方向でも、新カテゴリのアクション追加になるため ADR（Architecture Decision Record）の追加と AGENTS.md の改訂（UIA を使う場合）が必要

## 参考資料

- UI Automation TextPattern の実装ガイド: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementingtextandtextrange
- `ITextRangeProvider::ExpandToEnclosingUnit`: https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/nf-uiautomationcore-itextrangeprovider-expandtoenclosingunit
- 低レベルキーボードフック（`WH_KEYBOARD_LL`）: https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks
- xremap（Linux 用キーリマッパー、mark mode の先行例）: https://github.com/xremap/xremap
