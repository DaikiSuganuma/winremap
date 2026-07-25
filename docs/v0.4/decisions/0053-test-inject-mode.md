# ADR 0053: UI テスト自動化のための注入イベント受理モード（既定 OFF の Cargo feature）

- ステータス: 提案（オーナー承認待ち）
- 日付: 2026-07-25
- 作成: Claude Code（AI モデル: claude-opus-5）
- 参照: [`KBDLLHOOKSTRUCT`（Microsoft Learn）](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct)、[`SendInput`（Microsoft Learn）](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)、[Cargo features（The Cargo Book）](https://doc.rust-lang.org/cargo/reference/features.html)、[ADR 0015](../../v0.1/decisions/0015-menu-mask-key.md)・[ADR 0016](../../v0.1/decisions/0016-debug-key-logging.md)（`dwExtraInfo` マーカーと注入イベントの扱い）

## 文脈

VM（VMware）＋ Claude Code ＋ Terminator MCP（Model Context Protocol: AI が外部ツールを呼ぶための規格）でゲスト OS の UI を操作し、WinRemap の GUI を自動テストする環境が整った。手順は windows-utility リポジトリの `.ai-rules/rules/workflows/windows-ui-testing.md`（共通手順）と `docs/projects/20260723_Windowsアプリテスト環境構築/11_UIテスト自動化ガイド(WinRemapエージェント向け).md`（WinRemap 固有の差分）にある。

これで自動化できるのは設定画面の描画・トレイ操作・ログ表示・config 表示までで、**肝心のリマップ動作（例: Notepad で `C-h` → Backspace）だけが検証できない**。理由は不変条件 1 の実装そのもので、`handle_event`（`src/hook.rs`）は `LLKHF_INJECTED` の立ったイベントを**送り主を問わず無条件で素通し**する。Terminator の入力は `SendInput` なのでこのフラグが立ち、変換されずに素通りする。

このままだと、WinRemap の中核機能（キー変換の E2E）だけが永久に手動テストのままになる。受け入れチェックリスト（`docs/v0.4/03_acceptance-checklist.md`）の大半がこの種の項目である。

## 決定

1. **既定 OFF の Cargo feature `test-inject` を追加し、それを有効にしたビルドでのみ `--accept-injected` フラグを受け付ける。** feature 無効時、このフラグは他の未知の引数と同じくエラー（`unknown_argument`）になり、関連コードは 1 行もコンパイルされない。
2. **受理するのは「自分が注入したのではないイベント」だけとする。** `dwExtraInfo` が `MARKER_REMAP` / `MARKER_COMPENSATION`（`src/sender.rs`）のいずれかである注入イベントは、テストモードでも**従来どおり無条件で素通し**する。変換対象になるのは第三者（Terminator の `SendInput` 等）が注入したイベントのみ。
3. **モードの判定は `AtomicBool` の `load` 1 回で行う**（既存の `debug_enabled()` と同じ形）。フックコールバック内に確保・ロック・I/O・重い Win32 呼び出しを増やさない（不変条件 2）。`unsafe` も増やさない（不変条件 3）。
4. **このモードで起動したことを明示する。** 起動ログとトレイメニュー先頭に `TEST BUILD` を出し、通常ビルドと取り違えて常用できないようにする。文字列は `src/i18n.rs` を通す（v0.4 開発計画 §5）。
5. **リリース経路は変更しない。** CI・リリースワークフローは feature を渡さないため、配布物（Releases・winget）には従来どおりこのコードが入らない。README の利用者向け記述にも載せない（開発者向け機能であり、`docs/05_ui-test-automation.md` に書く）。
6. **キー入力内容の保存経路は増やさない**（不変条件 6）。このモードが変えるのは「どのイベントを変換するか」だけで、ログの粒度・保存先には触れない。

## 理由

- **不変条件 1 の目的は「自己送出ループの防止」であり、判定材料は `LLKHF_INJECTED` だけではない。** 自分の送出には既に `dwExtraInfo` のマーカーが付いており（ADR 0015・0016 で導入済み、`handle_event` の debug エコーで実際に判別している）、「マーカー付きは素通し」を保てばループは閉じる。**素通しの条件を緩めるのではなく、素通しの判定を「注入か」から「自分の注入か」へ精密化する**のがこの決定である。
- **通常ビルドの挙動を一切変えない担保が要る。** キーリマッパーが第三者の注入を変換する状態は、他の自動化ソフトと組み合わせたとき予期せぬ連鎖を生みうる。既定 OFF の feature なら、この危険はテスト用に明示ビルドした実行ファイルの中だけに閉じる。
- **`debug_assertions` ではなく feature を選ぶ理由**は、テスト対象を**本番と同じビルド条件**（release 最適化・静的 CRT＝[ADR 0052](0052-static-crt.md)）に保てること。debug ビルドを VM に置くと、検証しているバイナリが配布物と別物になり、起動時の依存 DLL 問題のような差分を自動テストが拾えない。
- **中核機能の E2E が回らないこと自体がリスク**である。現状、変換ロジックの単体テスト（注入判定を除いた部分）は通っても、「フック設置 → 抑止 → 送出 → アプリへの到達」の全経路は人手でしか確認できていない。自動化できれば受け入れチェックリストの反復コストが下がり、リリース前検証の抜けが減る。

## 却下した代替案

- **注入判定を撤廃 / 設定で切り替え可能にする**: 不変条件 1 の正面からの違反。他ソフトとの相互注入で無限ループに至りうる。テストのために製品の安全機構を緩めることはしない。
- **`#[cfg(debug_assertions)]` で debug ビルド限定**: feature を増やさず済むが、上記のとおり検証対象が配布物と別条件のバイナリになる。テスト環境ガイドも release exe を前提にしている。
- **マーカーの有無だけで判定し、feature を設けない**（通常ビルドでも第三者の注入を変換する）: コードは最も単純になるが、利用者の環境で他の自動化ソフト・リモートデスクトップ・IME の注入が変換対象になる。挙動の変化が大きすぎ、安定性優先の原則に反する。
- **UI 自動化を諦め、リマップ検証は単体テストのみで担保**: 現状維持。フック設置から送出までの経路は検証されないままで、受け入れテストの手動項目も減らない。
- **専用のテスト用バイナリ（別 `[[bin]]`）を作る**: 本体と別物になり、「テストしたものと配るものが同じ」という利点を失う。feature なら差分は 1 分岐に限定される。
- **キー入力をドライバレベル（Interception 等）で注入して `LLKHF_INJECTED` を立てずに送る**: 製品側の改修は不要だが、テスト環境にカーネルドライバの導入が要る。VM のゴールデンスナップショット運用が重くなり、環境依存の不安定要因を増やす。
