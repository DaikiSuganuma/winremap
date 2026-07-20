# ADR 0007: タスクトレイに tray-icon クレートを採用する

- ステータス: 承認
- 日付: 2026-07-18
- 作成: Claude Code（AI モデル: claude-fable-5）

## 文脈

Phase 3 でタスクトレイ常駐（有効/無効トグル、設定リロード、設定ファイルを開く、終了）を実装する。トレイはシェル API（`Shell_NotifyIconW`）と Win32 メニューを扱うため、実装手段の選定が必要。クレート追加はライセンス確認と ADR 記録が必須（03_rust-guidelines.md §8）。

## 決定

**`tray-icon` v0.24（tauri-apps、MIT OR Apache-2.0）を採用する**。メニューは同梱の muda（MIT OR Apache-2.0）を使う。

- イベントは `MenuEvent::receiver()` のチャネルを、既存のメッセージループの各メッセージ処理後に `try_recv` で排出する（追加スレッド・ロックなし、すべてメインスレッド）
- アイコンは 32x32 RGBA をコードで生成し、アセットファイルを持たない（無効時はグレー表示で状態を可視化）

## 理由

- **unsafe の隔離維持が決め手**: 自前で `Shell_NotifyIconW` + `CreatePopupMenu` を書くと `tray.rs` に大量の unsafe が必要になり、「unsafe は hook/sender/window のみ」（AGENTS.md 不変条件 3）に違反する。tray-icon なら `tray.rs` は unsafe ゼロ
- Tauri プロジェクトの一部として活発にメンテナンスされている
- 既存の Win32 メッセージループ（`GetMessageW`）とそのまま統合できる
- 推移的依存が小さい（muda / crossbeam-channel / once_cell / windows-sys）

## 却下した代替案

- **自前実装（Shell_NotifyIconW 直叩き）**: 依存ゼロだが unsafe 隔離違反。バルーン・DPI・Explorer 再起動時の再登録など既知のエッジケースを自前で背負うことになる
- **native-windows-gui**: GUI フレームワークとして大きすぎ、メンテナンス頻度も低い
- **trayicon クレート**: 機能は近いがメンテナンスが不活発で、tray-icon に比べ採用実績が少ない
