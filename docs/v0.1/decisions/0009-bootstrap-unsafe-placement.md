# ADR 0009: プロセス起動系 Win32 呼び出しの unsafe 配置

- ステータス: 承認
- 日付: 2026-07-18
- 作成: Claude Code（AI モデル: claude-fable-5）

## 文脈

ブリーフ §4 のモジュール表は「メッセージループ・単一インスタンス保証（named mutex）」を `main.rs` の責務とする一方、§5-3 は「unsafe は hook.rs / sender.rs / window.rs のみ」と定める。windows-rs では `GetMessageW` / `CreateMutexW` 等がすべて unsafe fn のため、この 2 つの規定は両立しない（Phase 2 の実装では `main.rs` にメッセージループの unsafe が置かれており、不変条件違反だった）。

## 決定

**§5-3（unsafe の 3 ファイル隔離）を優先**し、`main.rs` は unsafe ゼロを維持する。Win32 呼び出しは安全なラッパー関数として次のように配置する。

| 機能 | 配置 | 根拠 |
|---|---|---|
| メッセージループ（`run_message_loop`）、`PostQuitMessage` | `hook.rs` | LL フックはインストールしたスレッドのメッセージループ経由で配送される。ループはフック配送機構の一部 |
| 単一インスタンス mutex（`acquire_single_instance`） | `hook.rs` | 二重起動防止の目的は二重フック（挙動不定）の防止であり、フックの完全性を守る機構 |
| `UnhookWinEvent` ラッパー | `window.rs` | フォアグラウンド監視の解除は window.rs の設置 API と対 |

`main.rs` の責務は「起動フローのオーケストレーション（安全なラッパーの呼び出し順序）」と読み替える。Phase 3 でこの配置に是正済み。

## 理由

- unsafe を監査対象 3 ファイルに閉じ込める §5-3 の意図（レビュー面積の最小化）のほうが、モジュール表の字面より安全性への寄与が大きい
- ラッパーは所有権で後始末を表現でき（`SingleInstance` の Drop で `CloseHandle`）、main.rs 側の呼び忘れを型で防げる

## 却下した代替案

- **main.rs にも unsafe を許可するよう不変条件を緩める**: 不変条件の変更はオーナー承認事項であり、緩めなくても上記の配置で両立できる
- **bootstrap 専用の第 4 の unsafe ファイルを新設**: 監査対象ファイルが増える。機能的にも hook/window の責務で自然に説明できる範囲だった
- **single-instance 系クレートの追加**: 数行の named mutex のために依存を増やす価値がない
