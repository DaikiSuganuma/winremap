# ADR 索引（通し番号順）

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- 規約: ADR（Architecture Decision Record。設計判断の記録、1 判断 1 ファイル）は各バージョンフォルダの `decisions/` にあり、番号はバージョンをまたいだ通し番号（[docs/README.md](README.md)）。**新しい ADR を書いたら、この表に 1 行足す**（v1.1 開発計画 H4）。ステータスは各 ADR の冒頭が正で、ここは「承認／採用」「superseded」「一部上書き」の別だけを写す

| ADR | 題 | 版 | ステータス |
|---|---|---|---|
| [0001](v0.1/decisions/0001-use-windows-rs.md) | Win32 API バインディングに windows-rs を採用する | v0.1 | 承認 |
| [0002](v0.1/decisions/0002-rust-edition-2024.md) | Rust edition 2024・stable チャネルを採用する | v0.1 | 承認 |
| [0003](v0.1/decisions/0003-config-reload-arc-swap.md) | 設定リロードは arc-swap による atomic swap で行う | v0.1 | 承認 |
| [0004](v0.1/decisions/0004-keymap-matching-priority.md) | キーマップのマッチング意味論と優先順位 | v0.1 | 承認 |
| [0005](v0.1/decisions/0005-modifier-lift-restore.md) | 修飾キーの一時解除・復元シーケンス | v0.1 | 承認 |
| [0006](v0.1/decisions/0006-injection-markers.md) | 注入イベントの識別マーカー（dwExtraInfo 2 種） | v0.1 | 承認 |
| [0007](v0.1/decisions/0007-tray-crate-tray-icon.md) | タスクトレイに tray-icon クレートを採用する | v0.1 | 承認 |
| [0008](v0.1/decisions/0008-reload-via-tray-menu.md) | ホットリロードは v0.1 ではトレイメニューのみとする | v0.1 | 承認 |
| [0009](v0.1/decisions/0009-bootstrap-unsafe-placement.md) | プロセス起動系 Win32 呼び出しの unsafe 配置 | v0.1 | 承認 |
| [0010](v0.1/decisions/0010-icon-embedding.md) | アイコンは exe リソースとして埋め込み、トレイはリソースから読む | v0.1 | 承認 |
| [0011](v0.1/decisions/0011-application-exclude-list.md) | グローバルキーマップの除外リスト（exclude） | v0.1 | 承認 |
| [0012](v0.1/decisions/0012-macro-outputs.md) | マクロ出力（RHS のチョード配列） | v0.1 | 承認 |
| [0013](v0.1/decisions/0013-two-stroke-sequences.md) | 2 ストロークシーケンス（プレフィックスキー） | v0.1 | 承認 |
| [0014](v0.1/decisions/0014-ui-localization.md) | UI テキストの多言語化（日本語・英語） | v0.1 | 承認 |
| [0015](v0.1/decisions/0015-menu-mask-key.md) | Alt/Win のメニュー誤発動をマスクキーで防止する | v0.1 | 承認 |
| [0016](v0.1/decisions/0016-debug-key-logging.md) | --debug のキーイベントログ（リングバッファ経由） | v0.1 | 承認 |
| [0017](v0.1/decisions/0017-macro-minimal-mod-transitions.md) | マクロ出力は修飾キーを差分遷移で送出する | v0.1 | 承認 |
| [0018](v0.1/decisions/0018-opt-in-macro-pacing.md) | オプトインのマクロペーシング（--macro-delay） | v0.1 | 承認 |
| [0019](v0.1/decisions/0019-macro-delay-in-config.md) | macro_delay_ms を設定ファイルに昇格する | v0.1 | 承認 |
| [0020](v0.1/decisions/0020-ime-indicator-scope.md) | IME 状態インジケーターの本体統合（Non-goals の一部緩和） | v0.1 | 承認 |
| [0021](v0.1/decisions/0021-ime-indicator-trigger-keys.md) | IME インジケーターのトリガーキーを設定可能にする | v0.1 | 承認 |
| [0022](v0.1/decisions/0022-ime-indicator-getwindowrect.md) | インジケーターの位置計算は GetWindowRect に統一（DWM 矩形の不採用） | v0.1 | 承認 |
| [0023](v0.1/decisions/0023-ime-indicator-query-target.md) | インジケーターの照会対象ウィンドウの解決とシェル面の抑制 | v0.1 | 承認 |
| [0024](v0.1/decisions/0024-ime-indicator-app-name.md) | インジケーターへのアプリ名表示（opt-in、exe 名のみ） | v0.1 | 承認 |
| [0025](v0.1/decisions/0025-display-name-winremap.md) | 表示名を「WinRemap」表記に統一（技術識別子は小文字のまま） | v0.1 | 承認 |
| [0026](v0.1/decisions/0026-ime-indicator-reshow-after-shell.md) | シェル面経由でアプリに戻ったときはインジケーターを再表示する | v0.1 | 承認 |
| [0027](v0.1/decisions/0027-windows-installer-inno-setup.md) | Windows インストーラーを Inno Setup で追加（ユーザー単位・管理者権限不要） | v0.1 | 承認 |
| [0028](v0.1/decisions/0028-github-pages-help-site.md) | GitHub Pages によるヘルプサイト（手書き静的 HTML・Actions デプロイ） | v0.1 | 承認 |
| [0029](v0.2/decisions/0029-attach-console-and-tray-log-window.md) | 起動時コンソールの廃止と、AttachConsole + トレイ「ログ表示」ウィンドウによるログ出力 | v0.2 | 承認（決定 1 は ADR 0068 が上書き） |
| [0030](v0.2/decisions/0030-config-gui-framework-egui.md) | 設定 GUI のフレームワークに egui を採用する | v0.2 | 承認 |
| [0031](v0.2/decisions/0031-notify-module-unsafe-allowlist.md) | `src/notify.rs` を unsafe 隔離リストに追加する | v0.2 | 承認 |
| [0032](v0.2/decisions/0032-log-window-hide-instead-of-close.md) | ログウィンドウは閉じずに隠す（winit イベントループの再作成不可への対応） | v0.2 | 承認 |
| [0033](v0.2/decisions/0033-ime-status-across-input-threads.md) | IME 状態は前面アプリの入力スレッドすべてに問い合わせる | v0.2 | 承認 |
| [0034](v0.2/decisions/0034-ime-preedit-underline-visuals.md) | egui の未確定文字表示は下線ありにする（legacy_visuals = false） | v0.2 | 承認 |
| [0035](v0.2/decisions/0035-gui-single-event-loop-multi-viewport.md) | 設定 GUI とログウィンドウを 1 つの eframe アプリの複数ビューポートで動かす | v0.2 | 承認 |
| [0036](v0.2/decisions/0036-config-save-preserves-comments.md) | 設定の保存は toml_edit でコメント・書式を保持した部分更新にする | v0.2 | 承認 |
| [0037](v0.2/decisions/0037-gui-invisible-host-viewport.md) | eframe のルートビューポートは不可視のホストにし、実ウィンドウは全て子にする | v0.2 | 承認 |
| [0038](v0.2/decisions/0038-gui-win32-module.md) | GUI 用の Win32 モジュールを unsafe 許可リストに追加する | v0.2 | 承認 |
| [0039](v0.2/decisions/0039-macro-section-in-config.md) | マクロ設定を `[macro] delay_ms` に移す（旧 `macro_delay_ms` も受け付ける） | v0.2 | 承認 |
| [0040](v0.2/decisions/0040-menu-icons-rasterized-at-build-time.md) | トレイメニューのアイコンは Bootstrap Icons をビルド時にラスタライズする | v0.2 | 承認 |
| [0041](v0.2/decisions/0041-local-time-for-session-banners.md) | ログのセッション境界は Win32 のローカル時刻で記録する | v0.2 | 承認 |
| [0042](v0.2/decisions/0042-window-icons-white-and-tinted.md) | ウィンドウ内のアイコンは白で焼き、描画時に着色する | v0.2 | 承認 |
| [0043](v0.3/decisions/0043-runtime-macro-recording.md) | 実行時のキーボードマクロ記録・再生の採用（Non-goals の一部緩和） | v0.3 | 承認 |
| [0044](v0.3/decisions/0044-macro-replay-thread.md) | マクロ再生を専用スレッドで行う（不変条件 2・3 の改訂） | v0.3 | 承認 |
| [0045](v0.3/decisions/0045-package-manager-channels.md) | winget / scoop の配布チャネル方針 | v0.3 | 承認 |
| [0046](v0.3/decisions/0046-help-site-pico-css.md) | ヘルプサイトに Pico CSS を採用する（CDN 経由） | v0.3 | superseded by 0047 |
| [0047](v0.3/decisions/0047-help-site-bootstrap.md) | ヘルプサイトのフレームワークを Pico CSS から Bootstrap へ変更する | v0.3 | 承認 |
| [0048](v0.3/decisions/0048-scoop-defer-extras.md) | scoop 配布を保留する（Extras の知名度基準を満たすまで） | v0.3 | 承認 |
| [0049](v0.4/decisions/0049-config-edit-draft-model.md) | 設定 GUI 編集は「閲覧＝ライブ表 / 編集＝ファイル起点ドラフト」の二モデルで行う | v0.4 | 承認 |
| [0050](v0.4/decisions/0050-config-file-switching.md) | 設定ファイルの切り替えはパスの一本化＋既存リロード経路で行う | v0.4 | 承認 |
| [0051](v0.4/decisions/0051-notify-file-watch.md) | 設定ファイルの変更検知に notify クレートを使う（変更ありマーク） | v0.4 | 承認 |
| [0052](v0.4/decisions/0052-static-crt.md) | MSVC C ランタイムを静的リンクする（vcruntime140.dll 依存の排除） | v0.4 | 承認 |
| [0053](v0.4/decisions/0053-test-inject-mode.md) | UI テスト自動化のための注入イベント受理モード（既定 OFF の Cargo feature） | v0.4 | 承認 |
| [0054](v0.4/decisions/0054-rule-delete-keeps-comment-lines.md) | 規則を削除しても、その上のコメント行は残す | v0.4 | 承認 |
| [0055](v0.5/decisions/0055-accesskit-for-child-viewports.md) | 子ビューポートに AccessKit を付ける方式 | v0.5 | 承認 |
| [0056](v0.5/decisions/0056-control-codes-in-the-log.md) | ログに制御コードを表示する（印字可能文字は出さない） | v0.5 | 承認 |
| [0057](v0.5/decisions/0057-log-view-modes.md) | ログウィンドウを「判定」と「実況」の 2 モードにする | v0.5 | 承認 |
| [0058](v0.5/decisions/0058-log-readability.md) | ログを「読める」ものにする — キー名・タグ・コンソール出力の一本化 | v0.5 | 承認 |
| [0059](v0.6/decisions/0059-first-run-creates-the-default-config.md) | 初回起動時に既定の設定ファイルを作る | v0.6 | 承認 |
| [0060](v0.6/decisions/0060-msix-package.md) | Microsoft Store 向け MSIX パッケージの構成 | v0.6 | 承認 |
| [0061](v0.6/decisions/0061-packaged-config-path.md) | パッケージ実行時の設定ファイルパスを起動時に 1 回解決する | v0.6 | 承認 |
| [0062](v0.6/decisions/0062-detach-console-when-resident.md) | 常駐に入る直前にコンソールを切り離す（`--debug` を除く） | v0.6 | 承認 |
| [0063](v0.7/decisions/0063-symbol-keys.md) | 記号（OEM）キーは刻印の文字で書き、配列は Windows に聞く | v0.7 | 承認 |
| [0064](v0.7/decisions/0064-winapp-cli-for-ui-tests.md) | UI テストの操作系を winapp CLI にし、エージェント版シナリオを廃止する | v0.7 | 承認 |
| [0065](v0.7/decisions/0065-foreground-window-from-the-event.md) | 前面ウィンドウはイベントが運ぶ HWND で決める（引き直さない） | v0.7 | 承認 |
| [0066](v0.8/decisions/0066-zola-static-site.md) | ヘルプサイトを Zola で生成し、生成物をリポジトリにコミットする | v0.8 | 承認 |
| [0067](v0.8/decisions/0067-ime-cursor-color.md) | IME の状態をマウスカーソルの色で示す（`SetSystemCursor` と、異常終了の検出） | v0.8 | 承認 |
| [0068](v0.8/decisions/0068-debug-console.md) | `--debug` は自前のコンソールを開き、閉じられるまで残す（ADR 0029 の決定 1 を覆す） | v0.8 | 承認 |
| [0069](v0.8/decisions/0069-interactive-acceptance-harness.md) | 受け入れテストの対話ハーネス（チェックリストを読み、チェックリストへ書く） | v0.8 | 承認 |
| [0070](v0.8/decisions/0070-agent-led-acceptance.md) | 受け入れの対話は AI エージェントが司会する（ハーネスは道具に下ろす） | v0.8 | 承認 |
| [0071](v0.8/decisions/0071-debug-console-must-not-kill-the-app.md) | `--debug` の出力経路で常駐を殺さない（QuickEdit と、書き込み失敗の panic） | v0.8 | 承認 |
| [0072](v0.8/decisions/0072-cursor-keeps-tint-when-state-unknown.md) | IME の状態が「不明」のとき、カーソルの色は動かさない | v0.8 | 承認 |
| [0073](v0.9/decisions/0073-restore-from-a-snapshot-taken-at-startup.md) | カーソルの復元は起動時に採った複製から行い、空の着色は入れない | v0.9 | 承認 |
| [0074](v0.9/decisions/0074-egui-0-36-and-the-upstream-pr.md) | egui のフォークを 0.36.1 に載せ替え、wgpu を検証できないまま upstream PR を出す | v0.9 | 承認（決定 5 の外す条件は ADR 0088 が置き換え） |
| [0075](v0.9/decisions/0075-a-half-installed-tint-must-say-so.md) | 半分だけ着いた着色は、そう言わなければならない | v0.9 | 承認 |
| [0076](v1.0/decisions/0076-read-cursors-unscaled.md) | 拡大表示では、カーソルを DPI 非対応の文脈で読む | v1.0 | 承認 |
| [0077](v1.0/decisions/0077-remember-the-chosen-config-file.md) | 設定ウィンドウで選んだ設定ファイルを、次回の起動へ引き継ぐ | v1.0 | 承認 |
| [0078](v1.0/decisions/0078-address-bar-elides-the-path.md) | アドレスバーのパスは、行が測った残り幅に合わせて省略する | v1.0 | 承認 |
| [0079](v1.0/decisions/0079-tray-menu-names-the-config-file.md) | トレイのメニューは、いま効いている設定ファイルの名前を出す | v1.0 | 承認 |
| [0080](v1.0.1/decisions/0080-tray-icon-asks-for-the-small-metric.md) | トレイのアイコンは、通知領域が描くサイズを自分で指定して読む | v1.0.1 | 承認 |
| [0081](v1.0.1/decisions/0081-icon-must-not-depend-on-its-background.md) | アイコンは背景に依存しない絵にし、サイズは実 DPI から求める | v1.0.1 | 承認 |
| [0082](v1.0.1/decisions/0082-small-master-for-small-sizes.md) | 小さいサイズは専用のマスターから焼き、トレイが要求する全サイズを `.ico` に持つ | v1.0.1 | 承認 |
| [0083](v1.1/decisions/0083-invariants-as-tests.md) | 不変条件のうち機械で確かめられるものはテストにし、unsafe の許可リストは 1 つのファイルを正本にする | v1.1 | 承認 |
| [0084](v1.1/decisions/0084-split-unsafe-modules-by-file.md) | unsafe を含む大きなモジュールはディレクトリ化し、許可リストには unsafe を実際に含むファイルだけを載せる | v1.1 | 承認 |
| [0085](v1.1/decisions/0085-feature-thread-worker.md) | 機能スレッドの起動・停止とスレッドメッセージの配管を 1 か所ずつにする | v1.1 | 承認 |
| [0086](v1.1/decisions/0086-hook-decisions-as-a-pure-engine.md) | フックが 1 つのキーイベントで何をするかは、ライブラリの純粋な状態機械が決める | v1.1 | 承認 |
| [0087](v1.1/decisions/0087-remove-answered-probes.md) | 問いに答え終えた調査プローブは `examples/` から消す | v1.1 | 承認 |
| [0088](v1.1/decisions/0088-keep-the-eframe-fork.md) | eframe のフォークは持ち続け、egui のリリースごとに載せ替える手順を文書にする | v1.1 | 承認 |
