# WinRemap IME 状態インジケーター設計書

> IME（日本語入力）がオンになった瞬間に、アクティブウィンドウの中央へ半透明のマークを一時表示する機能の設計書。
> 採用判断の経緯は [ADR 0020](decisions/0020-ime-indicator-scope.md)、実施計画は [09_ime-indicator-plan.md](09_ime-indicator-plan.md) を参照。

- 作成日: 2026-07-19
- 作成: Claude Code（AI モデル: claude-fable-5）／レビュー・承認: オーナー
- 元資料: オーナー提供の調査資料「開発要件定義・実装仕様書：IME状態連動型インジケーター（ウィンドウ中央表示）」（2026-07-19 受領、リポジトリ外）。本書は必要な内容を取り込み自己完結とする

---

## 1. 目的とスコープ

### やること

- IME の状態が「オフ → オン」に変化した瞬間、および IME オンの状態でフォアグラウンドウィンドウが切り替わった瞬間に、アクティブウィンドウの中央へ半透明インジケーターを表示する
- 一定時間（既定 800ms）経過後に自動的に非表示（フェードアウト）にする
- インジケーターは入力・フォーカス・クリックを一切奪わない（後述の拡張スタイルで保証）

### やらないこと（本機能の Non-goals）

- **IME の制御・切り替え**（オン/オフの変更、変換モードの変更）。ブリーフ §3.3 のとおり引き続きプロジェクトの Non-goal
- キャレット（テキストカーソル）位置の追跡と、キャレット近傍への表示。技術的難易度が極めて高いため、元資料の方針どおり「ウィンドウ中央」で確定
- 変換モードの詳細表示（ひらがな/カタカナ/英数の判別）。v1 は「オンになった」ことだけを通知する
- TSF（Text Services Framework）による正式な状態取得。まず IMM32 照会＋フック補助で成立するか検証する（ADR 0020）

---

## 2. 採用判断の根拠（メリット・デメリット）

統合可否の検討時（2026-07-19）に評価した内容の記録。オーナーはこれを踏まえて統合を決定した。

### メリット

1. **常駐基盤をほぼ全部使い回せる**: 元資料が要求するメッセージループ、タスクトレイ常駐、単一インスタンス保証、TOML 設定＋ホットリロード、フォアグラウンド変化検知（`window.rs` の `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`）は WinRemap に既にある。単体プログラムをゼロから書くより実装量が少なく、常駐プロセスとトレイアイコンが 1 つで済む
2. **代替検知手段が自然に手に入る**: モダン IME で `IMC_GETOPENSTATUS` が機能しない場合のフォールバック（半角/全角キー等のグローバル監視）は、WinRemap が既に持つ WH_KEYBOARD_LL フックそのもの。単体プログラムならこのためだけに 2 本目のキーボードフックを常駐させることになる
3. **利用者層が一致する**: Emacs 風キーバインドで日本語入力する利用者は IME 状態の見失いに悩みがちで、リマッパーと同じ設定ファイル・同じトレイで管理できるのは自然。単一バイナリ配布の方針とも合う

### デメリット・リスク

1. **ブリーフの Non-goals と隣接する**: ブリーフ §3.3 は IMM32/TSF 領域を「複雑度が跳ね上がる」ため除外してきた。表示のみでも、Windows 11 22H2 以降のモダン Microsoft IME では `IMC_GETOPENSTATUS` が実際の状態に関わらず常に 0 を返す非互換が報告されており、環境によっては「以前のバージョンの Microsoft IME を使う」設定の案内が必要になり得る。「安定性 > 単純さ > 機能」の優先順位と緊張関係にある
2. **不変条件 2（フック内処理制限）との整合に設計が要る**: IME 状態の照会は他プロセスの IME ウィンドウへの `SendMessage` 系呼び出しで、相手が無応答だとブロックする。フックコールバック内からは絶対に呼べず、「フックは通知のみ → 専用スレッドで照会・描画」という経路の新設が必要
3. **初のウィンドウ描画サブシステムが入る**: レイヤードウィンドウ、フェードアウトのタイマー、DPI・マルチモニター対応は WinRemap にとって新領域で、unsafe を持つモジュールが増える。CI では一切テストできず、手動受け入れ項目が増える
4. **Windows Update で壊れうる依存が増える**: モダン IME の挙動は非公開仕様に近い。「リマッパーとしては安定しているのに IME 表示だけ壊れる」リリースが起きると信頼性の見え方に響く
5. **OS 標準機能との重複**: Microsoft IME には入力モード切り替え時に画面中央へ「あ」/「A」を表示する標準通知がある。差分は「アクティブウィンドウ中央への表示」と「表示時間・見た目の調整」

### リスクの抑え込み（設計への反映）

- 既定無効（opt-in）＋ 設定 1 行で完全停止できる構造にする（§4）
- 実装前に照会 API の実環境検証（Go/No-Go）を挟む（計画書 Phase I1）
- 障害分離: インジケーター機能の不具合がキーリマップ（フック）に波及しない構造にする（§6）

---

## 3. 機能仕様

### 3.1 表示トリガー

| トリガー | 検知経路 | 動作 |
|---|---|---|
| IME トグル操作 | キーボードフックがトグル候補キーの keydown を検知 → 専用スレッドへ通知 → 短い遅延（既定 50ms）後に状態照会 | 照会結果が「オン」かつ直前の既知状態が「オフ」なら表示 |
| フォアグラウンド切替 | `EVENT_SYSTEM_FOREGROUND`（既存の window.rs 監視）→ 専用スレッドへ通知 → 状態照会 | 照会結果が「オン」なら表示（切替先でオンだと気づけるように） |

- トグル候補キー（組み込み）: 半角/全角（`VK_KANJI` 0x19、JIS の `VK_OEM_AUTO` 0xF3 / `VK_OEM_ENLW` 0xF4）、`VK_IME_ON` 0x16 / `VK_IME_OFF` 0x1A、変換 `VK_CONVERT` 0x1C、無変換 `VK_NONCONVERT` 0x1D、カタカナ/ひらがな `VK_KANA` 0x15。修飾キーの状態は無視して VK のみで判定
- 追加トリガー（2026-07-19 追加、[ADR 0021](decisions/0021-ime-indicator-trigger-keys.md)）: IME 切替キーはユーザーが IME 設定で自由に割り当てられるため（例: Windows 11 IME の Ctrl+Space）、`trigger_keys` 設定でコマンド（修飾キー込み完全一致）を追加できる。Phase I3 のオーナー検証で Ctrl+Space 切替が検知されない問題が発覚し導入した
- トグルキー検知はあくまで「照会のきっかけ」であり、状態の正は常に照会結果とする（キー検知だけで状態を推測しない。マウスや他ツールによる切替はフォアグラウンド切替時・次回トグル時に追随）
- 照会前の遅延はトグルキー処理が IME に反映されるまでの時間を吸収するため。専用スレッド上のタイマーで行い、フックは待たない

### 3.2 IME 状態の照会（専用スレッド上でのみ実行）

1. `GetForegroundWindow` で対象ウィンドウの HWND を取得
2. （2026-07-19 追加、[ADR 0023](decisions/0023-ime-indicator-query-target.md)）ウィンドウクラスがシェル面なら表示・非表示を行わない。ただし直前の対象ウィンドウの記憶はリセットし、シェル面経由で元のアプリへ戻ったときに IME オンなら再表示する（同日改訂、[ADR 0026](decisions/0026-ime-indicator-reshow-after-shell.md)）。対象クラス: `Shell_TrayWnd` / `Shell_SecondaryTrayWnd`（タスクバー）、`Progman` / `WorkerW`（デスクトップ）、`NotifyIconOverflowWindow` / `TopLevelWindowForOverflowXamlIsland`（トレイのオーバーフロー。同日追記: 矢印クリックで表示される報告への対処）、`Shell_InputSwitchTopLevelWindow`（Win+Space の入力切替フライアウト）。UWP ホスト（ApplicationFrameHost 等）では直下の子 `Windows.UI.Core.CoreWindow` があればそちらを照会対象にする
3. `ImmGetDefaultIMEWnd(hwnd)` でデフォルト IME ウィンドウを取得
4. `SendMessageTimeoutW(ime_wnd, WM_IME_CONTROL (0x0283), IMC_GETOPENSTATUS (0x0005), 0, SMTO_ABORTIFHUNG, timeout=100ms)` を送信。戻り値が非 0 なら IME オンと判定
5. タイムアウト・失敗時は「不明」とし、表示しない（誤表示より非表示を優先）

**`SendMessage`（無印）は使用禁止**。相手プロセスが無応答の場合にこちらのスレッドがブロックするため、必ず `SendMessageTimeoutW` を用いる。

### 3.3 オーバーレイ表示

- 表示位置: アクティブウィンドウの中央。矩形は `GetWindowRect` で取得する（2026-07-19 改訂: 当初優先としていた `DWMWA_EXTENDED_FRAME_BOUNDS` は物理ピクセル座標を返し、DPI 非対応プロセスの仮想化座標と食い違ってスケーリング環境でパネルが画面外に出るため不採用。[ADR 0022](decisions/0022-ime-indicator-getwindowrect.md)）
- 表示内容: 半透明の角丸パネルに「あ」を 1 文字描画（v1 は固定デザイン。色・形状の設定化はしない）。`show_app_name = true` の場合はパネル下部に対象アプリの exe 名を小さく表示し、名前の幅に応じてパネル幅を size〜size×2.5 で自動調整する（2026-07-19 追加、[ADR 0024](decisions/0024-ime-indicator-app-name.md)。ウィンドウタイトルは機密になり得るため表示しない）
- ウィンドウ構成: レイヤードウィンドウ（`WS_EX_LAYERED`）＋以下の拡張スタイルを必須とする（元資料 §3.3）
  - `WS_EX_TRANSPARENT` — マウス入力を背後のウィンドウへ透過
  - `WS_EX_NOACTIVATE` — 表示時にフォーカスを奪わない
  - `WS_EX_TOPMOST` — 最前面表示
  - `WS_EX_TOOLWINDOW` — タスクバー・Alt+Tab に出さない
- 消灯: `SetTimer` で `duration_ms` 経過後にアルファ値を段階的に下げてフェードアウトし、非表示にする（ウィンドウは破棄せず再利用）
- DPI: Per-Monitor DPI awareness を前提にサイズを論理 px で計算する（マニフェストの現状確認は計画書のタスク）

### 3.4 設定（config.toml）

```toml
# 既定は無効。この節ごと省略可
[ime_indicator]
enabled = true              # 既定 false
duration_ms = 800           # 表示時間。許容範囲 100–5000
size = 96                   # パネルの一辺（論理 px）。許容範囲 32–256
opacity = 200               # 0–255。パネル全体の不透明度
trigger_keys = ["C-Space"]  # 追加の切替検知キー（ADR 0021）。既定 []
show_app_name = false       # パネル下に exe 名を表示（ADR 0024）。既定 false
```

- パース・検証は既存の config 層（ライブラリクレート側）に追加し、範囲外はキーマップ設定と同様に行番号付きエラーで報告する
- ホットリロード対応: リロード時、既存の arc-swap スナップショットと同じ経路で専用スレッドに新設定を通知する。`enabled = false` へのリロードでスレッドと通知は完全に停止する
- CLI フラグ・トレイメニューは v1 では追加しない（設定ファイルで完結）

---

## 4. アーキテクチャとソース分離

**本機能は既存のリマップ機能から独立した「別機能」であることをソース構成で明示する。** 新規コードはすべて `src/ime_indicator/` 配下に置き、既存モジュールへの接点は通知 1 行レベルに限定する。

```
src/
├── ime_indicator/            # ★ 本機能はこのディレクトリに完結（新規）
│   ├── mod.rs                #   公開 API: sync_with_config() / stop() / notify_*()。
│   │                         #   スレッド管理と状態機械（unsafe なしの制御ロジック）
│   ├── detect.rs             #   IME 状態照会（ImmGetDefaultIMEWnd + SendMessageTimeoutW）— unsafe
│   └── overlay.rs            #   レイヤードウィンドウの生成・描画・フェードアウトと、
│                             #   indicator スレッドのメッセージング補助 — unsafe はここに集約
├── ime_indicator_settings.rs # [ime_indicator] 設定型（ライブラリ側・純粋ロジック、CI テスト対象）
├── hook.rs                   # 接点1: トグル候補キー keydown で notify_toggle_keydown() 1 呼び出し
├── window.rs                 # 接点2: フォアグラウンド変化で notify_foreground_changed() 1 行
├── main.rs                   # 接点3: 起動時に sync_with_config()、終了時に stop()
└── tray.rs                   # 接点4: 設定リロード後に sync_with_config() 1 行
```

（2026-07-19 実装時修正: リロードはトレイメニュー経由のため、当初 main.rs に含めていた「リロード時の配線」は tray.rs の 1 行となり、接点は計 4 箇所。設定型はモジュール循環を避けるため `src/config/` 内ではなく独立モジュール `ime_indicator_settings.rs` とした）

### 4.1 スレッドモデル

- `ime_indicator::start()` は**専用スレッド（indicator thread）**を 1 本起動する。スレッドは自前のメッセージループを持ち、オーバーレイウィンドウとタイマーを所有する
- フック/メインスレッドからの通知は `PostThreadMessageW(WM_APP + n)` による非同期メッセージのみ。共有可変状態を持たない（設定スナップショットの arc-swap 参照を除く）
- indicator thread 上では待機・照会・描画・タイマーが自由に行える（フックスレッドではないため不変条件 2 の制約対象外）

```
[hook.rs]  トグル候補キー keydown（enabled 時のみ）
    │ PostThreadMessageW（非ブロッキング、フックはここまで）
    ▼
[indicator thread]  遅延タイマー → IME 状態照会（SendMessageTimeoutW）
    │ オフ→オン 遷移を検出
    ▼
[overlay.rs]  前面ウィンドウ中央にパネル表示 → duration_ms 後フェードアウト
```

### 4.2 既存アーキテクチャとの関係

- 3 層構造（フック層薄く・ロジック層厚く）は維持。本機能は「フック層に通知 1 行、ロジックは indicator thread」でこの原則に従う
- `[ime_indicator]` 設定のパース・検証はライブラリクレート（`winremap::config`）の純粋ロジックとして実装し、CI でテストする（フック層・描画層はテスト免除の既存方針どおり）

---

## 5. 不変条件との整合（AGENTS.md §技術的不変条件）

| 不変条件 | 本機能での扱い |
|---|---|
| 1. 自己送出ループ防止 | 影響なし。本機能はキーイベントを送出しない |
| 2. フック内処理制限 | フック内で行うのは「enabled かつトグル候補キーの keydown なら `PostThreadMessageW` 1 回」のみ。ADR 0016（--debug）と同型の軽量・非ブロッキング呼び出しであり、**第 3 の明示的例外として AGENTS.md / ブリーフ §5 の改訂が必要**（オーナーレビュー必須、計画書 Phase I0） |
| 3. unsafe の隔離 | `ime_indicator/detect.rs` と `ime_indicator/overlay.rs` を unsafe 許可リストに追加する（同上、AGENTS.md 改訂が必要）。`// SAFETY:` コメント必須は同様 |
| 4. 抑止と送出の順序 | 影響なし。トグル候補キーは**素通し**であり、抑止も置換もしない |
| 5. 既知の制限の明文化 | UIPI により管理者権限ウィンドウの IME 状態は取得できない → 表示されないことを README Limitations に追記 |
| 6. キーロガー化の禁止 | 本機能はキー内容を記録しない。扱う情報は「IME がオンか」のみ。ログ・永続化なし |
| 7. 文字コード | 既存の変換集約方針に従う |

### 障害分離（安定性 > 機能）

- indicator thread の panic・失敗は本機能の停止に留め、フック（リマップ本体）には波及させない。スレッド起動失敗時は警告を出してリマップのみで続行する
- 照会失敗・タイムアウトは「表示しない」に倒す。リトライループは持たない

---

## 6. 既知の課題と制限

1. **モダン IME 非互換（最重要リスク）**: Windows 11 22H2 以降の新 Microsoft IME では `IMC_GETOPENSTATUS` が常に 0 を返す環境が報告されている。実装前にオーナー環境で検証し（計画書 Phase I1、Go/No-Go）、機能しない場合の扱い（レガシー IME 案内 / 機能見送り / TSF 検討）はオーナー判断とする
2. **UIPI**: 管理者権限ウィンドウでは IME 状態を取得できず、インジケーターは表示されない。回避ハックは実装しない（不変条件 5 と同方針）
3. **フルスクリーン排他アプリ**: 排他モードのゲーム等では TOPMOST オーバーレイが表示されない場合がある
4. **表示位置の限界**: キャレット位置ではなくウィンドウ中央のため、視線移動は残る（仕様として許容。元資料の方針どおり）
5. **他の IME**（ATOK、Google 日本語入力等）: IMM32 互換レイヤーの挙動が Microsoft IME と異なる可能性がある。v1 は Microsoft IME のみをサポート対象とし、README に明記する

---

## 7. テスト方針

- **CI（自動）**: `[ime_indicator]` 設定のパース正常系・異常系（範囲外・型違い・節省略時の既定値）。純粋ロジックのみ
- **手動受け入れ**（`docs/05_acceptance-checklist.md` に追加する項目案）:
  1. IME をオンにするとアクティブウィンドウ中央にインジケーターが表示され、`duration_ms` 後に消える
  2. 表示中もタイピング・クリックが一切阻害されない（クリックが背後に透過する）
  3. 表示時にフォーカスが奪われない（入力先が変わらない）
  4. IME オンのままウィンドウを切り替えると切替先の中央に表示される
  5. `enabled = false` でリロードすると表示が完全に止まり、リマップ動作に影響がない
  6. 高速タイピング・キーリピート中に取りこぼし・二重入力が発生しない（既存項目の再確認）
  7. マルチモニター・DPI 混在環境で正しい位置・サイズに表示される

---

## 8. 参照（公式ドキュメント）

- ImmGetDefaultIMEWnd: https://learn.microsoft.com/en-us/windows/win32/api/imm/nf-imm-immgetdefaultimewnd
- WM_IME_CONTROL（IMC_GETOPENSTATUS を含む）: https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-control
- WM_IME_NOTIFY: https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-notify
- SendMessageTimeoutW: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendmessagetimeoutw
- SetWinEventHook: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook
- Extended Window Styles（WS_EX_LAYERED / TRANSPARENT / NOACTIVATE / TOPMOST / TOOLWINDOW）: https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
- SetLayeredWindowAttributes: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes
- DWMWINDOWATTRIBUTE（DWMWA_EXTENDED_FRAME_BOUNDS が物理ピクセルを返す根拠。ADR 0022 で不採用）: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
- Virtual-Key Codes（VK_IME_ON / VK_IME_OFF / VK_KANJI 等）: https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes
- IME 状態取得の非互換に関する Microsoft Q&A（元資料の引用）: https://learn.microsoft.com/en-us/answers/questions/5612690/detecting-the-ime-status-of-other-windows

---

*最終更新: 2026-07-19*
