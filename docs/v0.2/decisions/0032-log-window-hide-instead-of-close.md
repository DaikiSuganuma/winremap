# ADR 0032: ログウィンドウは閉じずに隠す（winit イベントループの再作成不可への対応）

- ステータス: accepted
- 日付: 2026-07-21
- 作成: Claude Code（AI モデル: claude-opus-4-8）／レビュー・承認: オーナー

## 文脈

ADR 0029 で決めたトレイの「ログを表示」を egui/eframe で実装したところ（ADR 0030）、
オーナーの実機確認で次の不具合が出た。

> ログ表示を一回閉じた後に再度開こうとすると、下記エラーが表示されました。
> `winit EventLoopError: EventLoop can't be recreated`

原因は winit の制約である。`EventLoop` は 1 プロセスにつき 1 回しか生成できず、
`eframe::run_native` は既定（`NativeOptions::run_and_return = true`）でウィンドウを
閉じるとイベントループを終了して返る。2 回目の `run_native` は必ずこのエラーになる。

参照:

- winit `EventLoop::new` のドキュメント（1 プロセス 1 回の制約）:
  <https://docs.rs/winit/0.30.13/winit/event_loop/struct.EventLoop.html#method.new>
- eframe `NativeOptions::run_and_return`:
  <https://docs.rs/eframe/0.35.0/eframe/struct.NativeOptions.html#structfield.run_and_return>

## 決定

**ログウィンドウを閉じてもイベントループを終了させず、ウィンドウを隠すだけにする。**
プロセス起動後に一度でも「ログを表示」を選んだら、イベントループスレッドはプロセス終了まで
生き続ける。

- 閉じる操作: `close_requested` を検知して `ViewportCommand::CancelClose` +
  `ViewportCommand::Visible(false)` を送る。同時にデバッグログを起動時の状態
  （`--debug` の有無）へ戻し、バッファを破棄する。利用者から見た挙動は「閉じた」ままである
- 再度開く操作: フラグを立てて `egui::Context::request_repaint()` でイベントループを起こし、
  次のフレームで `Visible(true)` / `Minimized(false)` / `Focus` を送る
- 隠れている間はフレームを要求しない（`request_repaint_after` を呼ばない）。スレッドは
  完全に眠り、CPU を消費しない

`request_repaint()` が隠れたウィンドウにも効くのは eframe 側の対応による。eframe 0.35 は
「Windows では不可視ウィンドウに `RedrawRequested` が届かないため、`Visible(true)` などの
viewport コマンドが処理されなくなる」問題（egui Issue #5229）に対し、不可視ウィンドウを
直接描画する経路を持つ。加えて Issue #7776 の対応として不可視時の再描画は 100ms に
スロットルされるため、ビジーループにもならない。

参照:

- egui Issue #5229（不可視ウィンドウで viewport コマンドが処理されない）:
  <https://github.com/emilk/egui/issues/5229>
- egui Issue #7776（不可視ウィンドウの再描画による CPU 使用率）:
  <https://github.com/emilk/egui/issues/7776>

## 理由

- **不変条件 1（フックを止めない）を最優先**。イベントループはフックのメッセージループとは
  別スレッドであり、開閉が本体に触れない構造を維持できる
- **unsafe を増やさない**。`ShowWindow` / `SetForegroundWindow` を生 HWND に対して直接呼ぶ
  案もあったが、`log_window.rs` を unsafe 許可リストへ追加する必要が生じる（不変条件 3）。
  egui の viewport コマンドで完結するなら、そのほうが規約上も単純である
- **隠すコストが小さい**。ウィンドウは再表示時に位置・サイズを保つため、利用者にとっては
  むしろ自然な挙動になる
- 隠れている間は描画要求を出さないので、常駐コストは眠っているスレッド 1 本のみ

## 却下した代替案

- **生 HWND に `ShowWindow` を呼ぶ**: 確実だが unsafe 許可リストの追加が必要で、
  ADR 0031 に続く 2 件目の例外になる。egui 側で完結する手段があるため不要と判断した
- **ウィンドウを画面外へ移動して常時可視にしておく**: 不可視ウィンドウが描画されない問題は
  避けられるが、タスクバーに残り、常時フレームを回すため CPU を消費する
- **2 回目以降は「再起動してください」と案内する**: 実装は最小だが、デバッグのたびに常駐を
  落とさせることになり、この機能の目的（ターミナルなしでログを見る）を損なう
- **`run_and_return = false` にする**: ウィンドウを閉じるとイベントループが
  `std::process::exit(0)` を呼ぶ実装であり、ログウィンドウを閉じただけで WinRemap 本体が
  終了してしまう。論外である
