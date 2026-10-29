# v1.1 開発計画 — リファクタリング（カテゴリー別の棚卸しと方向性）

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／承認: オーナー（**2026-10-29 承認**。未決事項への回答は §8）
- 前提: v1.0.1（2026-09-14 リリース）後の `develop`（`ddfb984`）。[AGENTS.md](../../AGENTS.md)、[01_project-brief.md](../01_project-brief.md)、[02_rust-guidelines.md](../02_rust-guidelines.md)、[04_git-branching.md](../04_git-branching.md)、ADR 0001–0082 を読んだうえで、`src/` 全ファイルを読んで書いた
- 略語: ADR（Architecture Decision Record。設計判断の記録、1 判断 1 ファイル）、LOC（Lines of Code。行数）、CI（Continuous Integration。push ごとに走る自動検査）

---

## 0. この版の位置づけ

**v1.1 は「中身を整える版」とする（仮置き。§8-1 — オーナー決定 2026-10-29: 「進めながら決める」。リファクタリングの施策は実施するが、機能を一切入れないかどうかはまだ確定していない）。** この計画の範囲では利用者から見える機能の追加・変更は行わない。設定ファイルの書式、CLI フラグ、ログの行、トレイ・設定ウィンドウの見た目は v1.0.1 と同じでなければならない。**「振る舞いが変わっていないこと」が受け入れ条件そのもの**である。

### なぜ今か

- v0.1（2026-07）から v1.0.1 まで 11 版、`src/` は 16,704 行になった。ブリーフ §4 が描いた 3 層（`hook` → `keymap`+`window` → `sender`）に、設定 GUI・IME インジケーター・マクロ記憶・カーソル着色が足され、**規約と実態が離れた**ところがある
  - [02_rust-guidelines.md §5](../02_rust-guidelines.md) の「1 ファイル 300 行超で構造化」に対し、2,322 行（`gui/config_window.rs`）・1,427 行（`i18n.rs`）・1,250 行（`config/draft.rs`）・1,145 行（`cursor.rs`）がある
  - [02_rust-guidelines.md §3](../02_rust-guidelines.md) は「unsafe は 3 ファイルのみ」と言い、[AGENTS.md](../../AGENTS.md) 不変条件 3 は 13 ファイル＋`examples/ime_probe.rs` を挙げ、[ブリーフ §5-3](../01_project-brief.md) はその中間（`cursor.rs`・`gui/win32.rs` の追記が無い）で止まっている。**同じ不変条件の正本が 3 つあり、3 つとも違う**
  - ブリーフは「矛盾があればブリーフが正」と言うが、実際に新しいのは AGENTS.md の方で、**正本関係が逆転している**
- 不変条件（unsafe の隔離、`// SAFETY:` の必須、lib の OS 非依存）は**文章とレビューだけで守られている**。違反しても CI は赤くならない
- 同じ形のコードが 2 か所にある。スレッドの起動・停止（`ime_indicator/mod.rs` と `macro_record/mod.rs`）、スレッドメッセージの配管（`ime_indicator/overlay.rs` と `macro_record/banner.rs`）、修飾キー接頭辞の文字列化（`keymap/mod.rs`・`keyname.rs`）
- フックの判断ロジック（`hook.rs` の `on_key_down` 136 行）が `thread_local` と `sender` の副作用に縛られていて、**2 ストローク・リピート・メニューガードといった中核の挙動に単体テストが無い**。ブリーフ §4「フック層は薄く、ロジック層は厚く」の原則から、いちばん厚いロジックが外れている

### この版で達成すること（3 つ）

1. **不変条件を文章からテストへ** — unsafe の隔離と SAFETY コメントを `cargo test` が確かめる。正本は 1 つ
2. **規約の粒度へ** — 300 行超のファイルを責務で分け、テストは `tests.rs` へ。公開 API と挙動は変えない
3. **重複と正本のずれを無くす** — 同じ理由で変わるコードは 1 か所に。ドキュメントのモジュール表・リポジトリ構成を現行化

---

## 1. 計測（2026-10-29、`develop` `ddfb984`）

### 1.1 カテゴリー別の行数

| カテゴリー | ファイル | LOC | 備考 |
|---|---|---:|---|
| A. コア（`winremap` ライブラリ） | `lib.rs`、`keymap/`（6 ファイル）、`config/`（7 ファイル）、`recorder.rs`、`ime_indicator_settings.rs` | 4,977 | keymap 1,485 / config 3,078 / recorder 328 / 設定 77 |
| B. フック層と Win32 境界 | `hook.rs` 905、`sender.rs` 459、`notify.rs` 367、`window.rs` 257、`keyname.rs` 251、`layout.rs` 191、`package.rs` 146、`clock.rs` 51 | 2,627 | |
| C. 機能スレッド | `cursor.rs` 1,145、`ime_indicator/`（968）、`macro_record/`（691） | 2,804 | |
| D. GUI・表示 | `gui/`（3,870。うち `config_window.rs` 2,322）、`i18n.rs` 1,427、`theme.rs` 314、`tray.rs` 277 | 5,888 | 全体の 35% |
| E. 起動 | `main.rs` | 408 | |
| 合計 | 39 ファイル | 16,704 | |

### 1.2 長い関数（70 行超。閉じ括弧の対応で機械計測）

| 行数 | 場所 | 関数 | 扱い |
|---:|---|---|---|
| 166 | `src/main.rs:56` | `run` | E1 |
| 136 | `src/hook.rs:710` | `on_key_down` | B3 |
| 136 | `src/gui/config_window.rs:1444` | `general_edit_ui` | D1 |
| 136 | `src/config/draft.rs:300` | `apply_general` | A4 |
| 116 | `src/ime_indicator/mod.rs:202` | `run` | C4 |
| 105 | `src/gui/log.rs:302` | `window_ui` | D4 |
| 104 | `src/hook.rs:598` | `handle_event` | B3 |
| 97 | `src/cursor.rs:434` | `apply` | C3 |
| 94 | `src/config/mod.rs:334` | `compile_ime_indicator` | A1 |
| 89 | `src/config/mod.rs:170` | `compile_macro_record` | A1 |
| 87 | `src/hook.rs:183` | `drain_debug_log` | B1 |
| 86 | `src/gui/config_window.rs:941` | `edit_list_ui` | D1 |
| 85 | `src/tray.rs:36` | `init` | D5 |
| 83 | `src/gui/config_window.rs:1073` | `keymap_edit_ui` | D1 |
| 82 | `src/config/draft.rs:468` | `rebuild_keymaps` | A4 |

### 1.3 プロセス全域の可変状態（`static` / `thread_local!` / `OnceLock` / `ArcSwap` の宣言数）

| ファイル | 件数 | ファイル | 件数 |
|---|---:|---|---:|
| `hook.rs` | 15 | `layout.rs` / `i18n.rs` | 4 / 4 |
| `gui/mod.rs` | 14 | `sender.rs` / `ime_indicator/overlay.rs` / `gui/watch.rs` / `cursor.rs` | 各 3 |
| `gui/log.rs` | 8 | `window.rs` / `package.rs` / `macro_record/banner.rs` / `ime_indicator/mod.rs` | 各 2 |
| `macro_record/mod.rs` | 6 | | |
| `notify.rs` | 5 | | |

`hook.rs` の 15 件はフック経路の性質上こうなる（`thread_local` でロック無し。ADR 0003・0016・0044）。問題は `gui/mod.rs` の 14 件で、`OnceLock<Mutex<..>>` を返す関数スロット（`loaded_stamp_slot` / `status_slot` / `config_path` / `config_memory` / `wake_context`）が 5 つ並び、**アプリの状態が 1 つの構造体ではなく 14 個の静的変数に散っている**（D3）。

### 1.4 `unsafe` の実使用（コメント中の語を除く `unsafe {` / `unsafe fn` / `unsafe extern`）

13 ファイル、125 ブロック: `cursor.rs` 24、`ime_indicator/overlay.rs` 19、`macro_record/banner.rs` 19、`hook.rs` 15、`notify.rs` 15、`window.rs` 9、`gui/win32.rs` 8、`ime_indicator/detect.rs` 7、`clock.rs` 2、`layout.rs` 2、`package.rs` 2、`sender.rs` 2、`keyname.rs` 1。ほかに `examples/ime_probe.rs` 11。

AGENTS.md 不変条件 3 の許可リストと**実態は一致している**（違反は無い）。一致していないのはドキュメント側どうし（§0）。

### 1.5 テスト

`#[test]` は 161 件（`config/tests.rs` 41、`keymap/tests.rs` 22、`config/draft.rs` 20、`keymap/control.rs` 10、`recorder.rs` 9、`tests/keymap_test.rs` 8、ほか）。CI は `windows-latest` なのでバイナリ側の純粋関数のテスト（`sender.rs` の `plan_transition` 等）も走っている。

**テストが無い中核**: `hook.rs` の状態機械（2 ストローク、スワロー、オートリピート、メニューガード、無効化トグル、記憶キーの優先）。フック層はテスト免除（ブリーフ §9）だが、それは「ロジックを持たせない」と対で成り立つ約束であり、現状はロジックがここにある。

**配置の不統一**: フォルダ化したモジュールの `#[cfg(test)]` は `tests.rs` に分離する規約（guidelines §5）に対し、`config/comments.rs`・`config/draft.rs`・`config/last_used.rs`・`keymap/control.rs` はインラインのまま。

### 1.6 再計測の方法

次の agent が同じ数字を出せるように残す（Git Bash）。

```sh
git ls-files src | xargs wc -l | sort -n                                   # 1.1
grep -cE "unsafe \{|unsafe fn|unsafe extern|unsafe impl" src/*.rs src/*/*.rs   # 1.4
grep -nE "^\s*(pub(\(crate\))? )?static |thread_local!|OnceLock|LazyLock|ArcSwap" src/*.rs src/*/*.rs | cut -d: -f1 | sort | uniq -c   # 1.3
grep -c "#\[test\]" src/*.rs src/*/*.rs tests/*.rs                        # 1.5
```

長い関数は、`fn` 行から `{` と `}` の対応が閉じるまでを数えた（awk。70 行超を抽出）。

---

## 2. カテゴリー別の棚卸し

各カテゴリーで「守るもの」「問題」「方向性」を書く。施策の ID（A1、B3 …）は §4 の一覧に対応する。

### A. コア（`winremap` ライブラリ: `keymap` / `config` / `recorder` / `ime_indicator_settings`）

**守るもの**
- OS 非依存の純粋ロジックであること（`windows` クレートを参照しない。ブリーフ §9）。`Layout` をスナップショットの値で受け取る設計（ADR 0063）は、テストが US と JP の 2 配列を同一プロセスで回せる根拠になっている
- `config` の「全問題を 1 回で行番号付きで報告する」設計（config-spec §4）。`raw`（serde）→ `compile`（検証）の分離
- `draft`（ADR 0036・0049）の「編集した箇所だけ書き換え、他は 1 文字も動かさない」保証と、それを守る 20 件のテスト

**問題**
- `config/mod.rs`（427 行）が「API と型」のほかに `compile_macro_delay` / `compile_macro_record` / `compile_ime_indicator` / `check_record_key_collisions`（計 250 行）を抱えている。モジュール冒頭の説明は「compile が検証と変換」と言っており、置き場所が説明と違う
- `config/draft.rs`（1,250 行）は読み（`parse` 側）と書き（`apply` 側、`apply_general` 136 行・`rebuild_keymaps` 82 行）の 2 責務にテスト 350 行が同居
- 修飾キー接頭辞の文字列化（`C-` `A-` `S-` `W-`）が `keymap/mod.rs:modifier_prefixes`（非公開）と `keyname.rs:combo` の 2 か所にある。`i18n.rs:combo` は `keyname::combo` の薄い委譲で、名前が同じ関数が 3 層に重なっている
- `lib.rs` の doc コメントに「Win32 層は Phase 2 で来る」とあり、v0.1 のまま

**方向性**: 置き場所の是正と分割のみ。型と公開 API は変えない。接頭辞の文字列化は `keymap` に 1 つ公開して他はそれを使う。

### B. フック層と Win32 境界（`hook` / `sender` / `window` / `layout` / `keyname` / `clock` / `package` / `notify`）

**守るもの**
- フック経路のゼロアロケーション・ロック無し（不変条件 2）。`thread_local` の固定長配列、`ArcSwap` の load のみ、リングバッファで message loop へ渡す形（ADR 0003・0016・0044）
- 注入イベントの素通しとマーカー（不変条件 1、ADR 0006・0053）。`test-inject` の `#[cfg]` 構造
- `window.rs` の「イベントが運ぶ hwnd を使い、`GetForegroundWindow` を呼ばない」（ADR 0065）
- `sender.rs` の修飾キー遷移の差分計算（ADR 0017）と、その純粋部分にテストがあること

**問題**
- `hook.rs`（905 行）に 5 つの責務が同居している: (1) コールバックと状態機械、(2) デバッグイベントのリングと**整形**（`drain_debug_log` 87 行は `i18n` を呼んで文字列を組む — 表示の仕事）、(3) 記憶イベントのリングと `macro_record` への配送、(4) 単一インスタンスのミューテックス、(5) メッセージループと wake。unsafe を 1 ファイルに閉じるためにここに集まったが、unsafe を含むのは (1)(4)(5) だけで、(2)(3) の整形・配送は安全なコードである
- `window.rs:print_debug_info`（`REMAP_TABLE` を読んでキーマップ名を列挙しログに出す）は表示の仕事で、Win32 境界のファイルにある
- `on_key_down`（136 行）と `handle_event`（104 行）が判断と送出を混ぜている。「このキー押下で何をするか」は `(状態, 表, 入力) → 決定` の純粋関数にできるが、いまは `thread_local` を直接読み書きし `sender::*` を呼ぶため、**単体テストで押下列を流して確かめられない**
- `notify.rs` の状態が 4 つの `AtomicBool`（`HAS_CONSOLE` / `ATTACHED` / `LAUNCHER_OWNS_OUTPUT` / `OWNS_CONSOLE`）に分かれ、「いまコンソールはどの状態か」が 1 か所で読めない。ADR 0029 → 0062 → 0068 と積み上がった経緯どおりの形で、遷移が読み取りにくい
- UTF-16 への変換が `HSTRING::from`（`notify.rs`・`gui/win32.rs`）と `encode_utf16().collect()`（`overlay.rs`・`banner.rs`）に散っている。ブリーフ §5-7 は「1 か所に集約」

**方向性**: `hook.rs` をディレクトリ化して unsafe を含む部分と含まない部分を分け、判断ロジックを lib の純粋関数へ移す（B3。この版の最大の施策で、最後に・受け入れ付きで行う）。`sender.rs` の修飾キー代数（`SideMods`、`plan_transition` 等）は B3 の前提として先に lib へ移す（B4）。

### C. 機能スレッド（`ime_indicator/` / `macro_record/` / `cursor.rs`）

**守るもの**
- 「機能はスレッドに閉じ、フックからは有界な `PostThreadMessageW` 1 回だけ」（不変条件 2 の例外 3・4）。`catch_unwind` でパニックを機能内に封じ、フックを止めない設計
- `mod.rs` は orchestration のみで unsafe を含まず、Win32 は `detect` / `overlay` / `banner` に閉じる構造
- バナーとインジケーターの**見た目を統合しない**判断（マクロ記憶設計書 §6.1。保持時間・文字量・配置が違う）
- `cursor.rs` の「着色は IME オンの間だけ、復元は起動時に無条件、クラッシュ経路でも復元」（ADR 0067・0073・0076）と、`Trouble` で失敗を飲み込まない方針

**問題**
- **スレッドのライフサイクルが 2 か所で同じ**: `ime_indicator/mod.rs:151-196` と `macro_record/mod.rs:512-557` の `start_thread` / `thread_main` / `stop`（ready チャネルで tid を受け取る、`THREAD_ID` / `THREAD_HANDLE` の静的変数、死んだ旧スレッドの join、`catch_unwind`）は変数名まで一致する
- **スレッドメッセージの配管が 2 か所で同じ**: `overlay.rs:384-431` と `banner.rs:325-360` の `current_thread_id` / `post_to_thread` / `post_quit_to` / `next_thread_message` は同一コード（overlay はタイマー 2 関数が多いだけ）。unsafe の重複でもある
- `cursor.rs`（1,145 行）に、純粋なピクセル演算（`outlined` / `and_mask` / `tint`。Win32 なし、テストあり）、GDI の読み書き（`dib_section` / `read_as_bgra` / `decode`）、セッション状態の置換・復元（`SetSystemCursor` / `SPI_SETCURSORS` / `Unscaled`）、公開 API と `Trouble` の報告が同居。unsafe 24 ブロックの位置が追いにくい
- `ime_indicator::run`（116 行）はタイマー管理・IME 状態の評価・パネルとカーソルの表示判断・デバッグログが 1 つの `match` 腕にある。「この sample と設定と前回状態で、表示するか・カーソルをどうするか」は純粋に決められる

**方向性**: ライフサイクルと配管を各 1 か所に。見た目のコードは触らない。`cursor` は責務で 4 分割し、純粋部分を lib へ出せるなら出す。

### D. GUI・表示（`gui/` / `theme` / `i18n` / `tray`）

**守るもの**
- 単一イベントループ＋不可視ホスト＋子ビューポート（ADR 0035・0037）。`gui/mod.rs` の doc コメントにある「フックは GUI を触らない」「ループは二度と作れない」の制約
- `theme.rs`（guidelines §12。色は関数、寸法は定数）
- `i18n.rs` の「両言語を同時に定義」。`Texts` 構造体はフィールド欠落が型エラーになる形で、これが片言語追加漏れを防いでいる
- `gui/log.rs` の「ログ行は 1 か所（`record`）で作る」（ADR 0057・0058）
- `tray.rs` のメニュー項目を**作成順**で ID が決まる注意書き（ADR 0079 影響・補足）。UI テストが ID 依存

**問題**
- `gui/config_window.rs`（2,322 行、68 関数）が 1 ファイル。アドレスバーと `FileList`、閲覧（`general_ui` / `keymap_ui` / `rules_ui` / `ime_ui`）、編集（`EditState` / `save` / `general_edit_ui` 136 行 / `keymap_edit_ui` / `notation_cell` / 前面キャプチャ）、ヘルプ（`notation_help_ui` / `key_names_help`）、表の部品（`table` / `section` / `field` / `note`）が混在。v0.2 設計書・v0.4 画面設計書は画面ごとに分かれているのに、コードは分かれていない
- `i18n.rs`（1,427 行）は `Texts` 120 フィールド＋約 70 の整形関数。「トレイ」「設定ウィンドウ」「ログ」「マクロ記憶」「CLI ヘルプ」の文言が 1 ファイルに並び、1 画面の文言を直すのに 1,400 行を往復する
- `gui/mod.rs` の状態が 14 個の静的変数（§1.3）。`choosing_a_file_records_it_for_the_next_start` テストは「この OnceLock を set する唯一のテスト」という前提で成り立っていて、2 つ目のテストが書けない
- `gui/log.rs:window_ui`（105 行）はツールバー・行描画・コピーが 1 関数
- `tray::init`（85 行）はメニュー構築が手続きのまま。作成順の制約はコメントで守られている

**方向性**: 分割のみ。見た目・文言・ID 順序は変えない。`i18n` は**ファイルを分けても「1 関数の中で両言語を `match`」の形は崩さない**（分けると片言語の追加漏れが型で捕まらなくなる）。

### E. 起動（`main.rs`）

**守るもの**
- `run()` の順序とその理由（コメントで逐一説明されている。コンソール接続 → カーソル復元の登録 → 言語 → 設定ファイルの決定 → 単一インスタンス → バナー → 読み込み → フック → トレイ → 機能スレッド → コンソール切り離し → ループ）。この順序の根拠が ADR 0029・0062・0067・0068・0073・0077 に分散している
- CLI パーサーを自前で持つ判断（依存最小。`clap` を入れない）
- `parse_args` が副作用を持たない設計と、そのテスト

**問題**
- `run()` 166 行。順序の制約があるので関数に分けても呼び出し順は残るが、「設定ファイルをどれにするか」（`memory` / `recalled` / `chosen` / `resolve_config_path`、約 30 行）と「常駐の準備」（フック・トレイ・スレッド）は段階として切り出せる
- CLI（`CliArgs` / `parse_args` / `extract_lang` / `default_config_path` / `memory_path`、約 150 行）は純粋でテスト済みなので、`cli.rs` に出せば `main.rs` は起動順序だけの 100 行になる

**方向性**: 段階関数への分割と CLI の分離。順序とコメントは残す。

### F. テスト（単体・結合・`tests/ui`・`tests/acceptance`）

**守るもの**
- `examples/*.toml` を結合テストの入力にし「サンプルは常にパースできる」を保証する形（`tests/keymap_test.rs`）
- `cursor.rs` のテストの「構築するだけで、インストールはしない」規則（ADR 0073 決定 6。セッション状態の置換は受け入れプローブの仕事）
- `tests/ui/`（VM ＋ winapp CLI、ADR 0064）と `tests/acceptance/`（対話ハーネス、ADR 0069・0070）。どちらも振る舞いの回帰検出として本計画の安全網になる

**問題**
- **不変条件がテストになっていない**: unsafe の隔離、`// SAFETY:` の必須、lib の OS 非依存は、いずれも `grep` で機械的に確かめられるのに CI には無い
- テストの配置が不統一（§1.5）
- `tests/ui/guest/probe-winapp.ps1` と `winapp-helpers.ps1` に同じ注意書き（PowerShell 5.1 の UTF-8 → CP932 問題）があり、コードも重複している可能性がある。**この版では調査のみ**（書き直しは対象外。§9）

**方向性**: 不変条件テストを最初に入れ、以降のコード移動が CI で守られる状態にしてから分割を始める。

### G. ビルド・依存・CI（`Cargo.toml` / `build.rs` / `.github/workflows` / `examples/`）

**守るもの**
- `build.rs` のアイコン埋め込みと SVG の焼き込み（ADR 0010・0040）。実行時コストゼロ
- `test-inject` を CI で clippy・test する並列ステップ（ADR 0053 の「腐らせない」）
- `site/` の生成一致チェック（ADR 0066）
- `windows` クレートの feature 最小化、静的 CRT（ADR 0052）

**問題**
- `Cargo.toml` の `[patch.crates-io]`（eframe フォーク）には「upstream に入ったら消す」とあるが、[ADR 0074](../v0.9/decisions/0074-egui-0-36-and-the-upstream-pr.md) の追記どおり **PR #8410 はクローズされた**。「一時しのぎ」の前提が消え、egui のリリースごとに rebase し続けるのか、子ビューポートの AccessKit を諦めて UI テストの方式を変えるのかが決まっていない。**コードのリファクタリングではないが、v1.1 の間に方針を文書化しておかないと `cargo update` のたびに同じ判断を迫られる**
- `examples/` に性格の違うものが 3 種類ある: 道具（`app_icons.rs`・`msix_assets.rs`。ADR 0082・0060 で使う）、調査プローブ（`accesskit_probe.rs`・`egui_ime_probe.rs`・`ime_probe.rs`。それぞれ v0.5・v0.2・v0.1 で問いに答え終えている）、サンプル設定（`*.toml`・`registry/`）。`ime_probe.rs` は unsafe 許可リストに載っていて、消せばリストが 1 行短くなる
- `winit` の直接依存は `with_any_thread` の 1 トレイトのため（`gui/mod.rs:271`）。eframe と版を揃える注意はコメントにあるが `Cargo.toml` には無い

**方向性**: 依存は入れ替えない。プローブの整理と eframe 方針は ADR で決める。

### H. ドキュメント（`AGENTS.md` / `docs/` 直下 / ADR）

**守るもの**
- 「指示ソースの限定」と不変条件 7 項目の内容。バージョンフォルダと ADR の規約（`docs/README.md`）
- 開発計画・受け入れチェックリスト・ADR が、オーナー報告の日付と結び付いて書かれていること（本計画もそれに倣う）

**問題**
- **unsafe 許可リストの正本が 3 つ**（§0）。AGENTS.md 不変条件 3 は 1 段落に 13 ファイルと 11 本の ADR が詰まっていて、ファイルを 1 つ足すたびにこの段落とブリーフ §5-3 の両方を直す必要があるが、ブリーフ側は 2 回（ADR 0067・0080）直されていない
- ブリーフ §4 のモジュール表に `gui/`・`cursor`・`package`・`macro_record`・`ime_indicator`・`keyname`・`i18n`・`theme`・`recorder` が無い。§7 のリポジトリ構成は `docs/decisions/`・`02_development-plan.md`・`config.rs`・`keymap.rs` と、v0.1 以前の姿のまま
- `02_rust-guidelines.md §3` の「3 ファイルのみ」は v0.1 の記述
- ADR が 82 本・11 フォルダに分かれ、**番号から場所とステータスを引く索引が無い**。「superseded」は 2 本だけで、上書きした ADR（例: 0068 が 0029 の一部を上書き）が旧 ADR のステータスに反映されているか追えない
- `docs/README.md` のバージョン表が v0.9 と v1.0 の間の空行で 2 つの表に割れている（表示上の不具合）
- `lib.rs` の doc コメント（A）

**方向性**: 正本を 1 つに決め、他は参照にする。ADR の索引を作る。

---

## 3. リファクタリングの原則（この版の方向性）

AGENTS.md の優先順位（安定性 > 単純さ > 機能の豊富さ）を前提に、この版だけの原則を置く。

| # | 原則 | 具体的に |
|---|---|---|
| P1 | **振る舞いを変えない** | 送出するキーイベント列、ログの行、設定ファイルの書き戻し結果、トレイ・ウィンドウの表示、CLI の出力を v1.0.1 と同一に保つ。「ついでに直す」をしない。見つけた不具合は Issue 相当のメモに残し、別ブランチで直す |
| P2 | **不変条件は機械で守る** | F1 のテストを最初に入れる。unsafe を含むファイルを増減する施策は、同じブランチで許可リストと AGENTS.md を直す |
| P3 | **フックは薄く、判断は純粋関数へ** | ブリーフ §4 の原則をフックの状態機械にも適用する（B3）。ただしアロケーション・ロック無しの性質は引き継ぐ（固定長の決定を返す） |
| P4 | **規約の粒度へ** | 300 行超は責務で分ける。フォルダ化したモジュールのテストは `tests.rs` へ。分けた後のファイルも 300 行前後を目安にする |
| P5 | **統合するのは「同じ理由で変わるもの」だけ** | スレッドの起動・停止と配管は統合する。バナーとインジケーターの見た目は統合しない（設計書 §6.1）。似ているだけの `match` は残す |
| P6 | **正本は 1 つ** | unsafe 許可リスト、モジュール表、修飾キー接頭辞の文字列化。他は参照か委譲 |
| P7 | **1 ブランチ 1 施策、移動と変更は別コミット** | `git` の rename 検出が効く粒度で「ファイルを動かしただけ」のコミットを先に作り、変更はその後に積む。レビューで差分が読める。`--no-ff` で戻せる |
| P8 | **やらないこと** | 機能追加、依存の入れ替え（eframe・winit・toml）、ワークスペース分割、PowerShell テストの書き直し、`site-src/`、フック経路の性能特性の変更 |

---

## 4. 施策一覧

優先: ★★★ = この版で必ず / ★★ = 入れる / ★ = 余力があれば。リスク: 低 = 移動のみ・テストで担保 / 中 = ロジックに触る / 高 = フック経路に触る。ADR: 設計判断を伴うもの（AGENTS.md ワークフロー 2）。

### A. コア

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| A1 | `config/mod.rs` の `compile_*` を `compile.rs` へ | ★★ | 低 | 不要 | `mod.rs` は `Issue` / `ConfigError` / `load` / `parse_str` / `create_default` のみに。`compile.rs` が 300 行を超えるなら `compile/{keymap,macro,ime}.rs` |
| A2 | 修飾キー接頭辞の正本を `keymap` に 1 つ | ★★ | 低 | 不要 | `modifier_prefixes` を `pub` にし、`keyname::combo` と `i18n::combo_human` の接頭辞部分はこれを使う。`i18n::combo`（委譲だけ）は削る |
| A3 | インラインテストを `tests.rs` へ | ★★★ | 低 | 不要 | `config/{comments,draft,last_used}.rs`、`keymap/control.rs`。`config/tests.rs` が肥大するなら `config/tests/{mod,draft,comments}.rs` |
| A4 | `config/draft.rs` を読みと書きで分割 | ★★ | 低 | 不要 | `draft/mod.rs`（型・`read`・`parse`・`stamp`）、`draft/apply.rs`（`apply` / `apply_general` / `rebuild_keymaps` / `rewrite_*`）、`draft/toml.rs`（`output_value` / `rule_key` / `ensure_table` 等の `toml_edit` 操作）。`apply_general` は節ごと（macro / ime）の関数に |
| A5 | `lib.rs` の古い doc を現行化 | ★★★ | 低 | 不要 | H6 と一緒に |

### B. フック層と Win32 境界

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| B1 | `hook.rs` をディレクトリ化 | ★★★ | 中 | **要** | `hook/mod.rs`（install / callback / `thread_local` 状態 / `REMAP_TABLE` / enabled）、`hook/debug.rs`（`DebugRing` / `drain_debug_log` / `DebugAction`。unsafe は wake の 1 行のみ）、`hook/record.rs`（`RecordEventRing` / `drain_record_events` / `abort_recording`）、`hook/instance.rs`（`SingleInstance`）、`hook/message_loop.rs`（`run_message_loop` / `wake_message_loop` / `post_quit`）。許可リストは「`hook.rs`」→「`src/hook/`」。ADR はこの許可範囲の変更と「unsafe を含まないファイルもディレクトリ内に置く」理由 |
| B2 | `window.rs:print_debug_info` を `hook/debug.rs` へ | ★★ | 低 | B1 と同一 | `window.rs` は Win32 呼び出しとキャッシュだけになる。`set_foreground_cache` は `Option<&str>` を返し、整形は呼び出し側 |
| B3 | **フック判断の純粋化（engine）** | ★★ | **高** | **要** | lib に `engine` モジュール。`struct HookState { sides, active: [Option<ActiveKind>; 256], pending, menu_guard, phys_down }`、`fn on_key(&mut self, table: &RemapTable, exe: &str, ev: KeyEvent, recorder: &mut Recorder, record_keys: Option<&RecordKeys>) -> Decision`。`Decision` は `Copy` で、`suppress: bool` と `emit: Option<Emit>`（`ExactDown` / `ExactRepeat` / `ExactUp` / `KeyOnly` / `MaskedModifierUp` / `Sequence(&[KeyCombo])`）、`debug: Option<DebugAction>`、`record: Option<RecordNote>`、`ime_poke: bool` を持つ。**アロケーション無し**（配列と `Copy` 型のみ）。`hook/mod.rs` は `Decision` を `sender` に流すだけ。テスト: 2 ストロークの成立と不成立（スワロー）、リピート中の修飾キー変化、Alt/Win メニューガード、無効化中のリピート継続、記憶キーの優先、注入イベント（マーカー別）の扱い、`test-inject` の `accept_injected`。**M4 で、受け入れ（実キーボード）付き。間に合わなければ v1.2**。ADR は不変条件 2 をどう担保するか（`Decision` の設計）と、lib に置く理由 |
| B4 | `sender.rs` の修飾キー代数を lib へ | ★★ | 中 | B3 と同一 | `SideMods` / `side_bit` / `side_mods_to_mods` / `sides_to_lift` / `plan_transition` / `CLASS_LEFT` / `SIDE_VKS` / `is_extended` を `keymap::sides` へ。`sender.rs` は `Batch` と `SendInput` だけ（約 200 行）。既存テスト 4 件はそのまま移る。B3 の前提 |
| B5 | `notify.rs` のコンソール状態を 1 つの型に | ★ | 中 | 不要 | 4 つの `AtomicBool` を `enum Console { None, Attached { launcher_owns_output }, Owned, Redirected }` 相当の 1 状態に。遷移（attach → detach / open_debug）が関数 1 つずつで読める形。挙動は同一。`--debug` と `> out.txt` の両方で受け入れ |
| B6 | UTF-16 変換の集約 | ★ | 低 | 不要 | `to_wide(&str) -> Vec<u16>` を 1 か所（`gui/win32.rs` か新設の小モジュール）に置き、`overlay.rs`・`banner.rs` の `encode_utf16().collect()` を置き換える。`HSTRING` の利用は `windows` クレートの型なのでそのまま |

### C. 機能スレッド

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| C1 | スレッドのライフサイクル共通化 | ★★★ | 中 | **要** | `src/worker.rs`（unsafe なし）に `Worker` 型: `start(name, build: FnOnce() -> Result<T>, run: FnOnce(&T))`、`stop()`、`thread_id()`。ready チャネル・`catch_unwind`・旧スレッドの join・失敗時の `notify::error` を 1 実装に。`ime_indicator/mod.rs` と `macro_record/mod.rs` は各 50 行減る。ADR は「機能スレッドの共通契約」として |
| C2 | スレッドメッセージ配管の共通化 | ★★★ | 中 | C1 と同一 | `current_thread_id` / `post_to_thread` / `post_quit_to` / `next_thread_message` / `set_thread_timer` / `kill_thread_timer` を `src/thread_msg.rs`（新規、unsafe 許可リストに追加）へ。`overlay.rs` と `banner.rs` から同コードを削除（各 19 ブロックのうち 5〜7 が消える）。**窓・描画は触らない** |
| C3 | `cursor.rs` を 4 分割 | ★★ | 中 | **要** | `cursor/mod.rs`（公開 API・`Trouble`・`Action`・静的状態）、`cursor/pixels.rs`（`outlined` / `and_mask` / `tint`。純粋。lib `winremap::pixels` へ出す案は ADR で判断）、`cursor/gdi.rs`（`dib_section` / `read_as_bgra` / `decode` / `drawn_pixels` / `Bitmap`）、`cursor/system.rs`（`load` / `snapshot` / `tinted` / `restore_inner` / `reload_from_registry` / `Unscaled` / `install_crash_restore`）。許可リストは「`src/cursor.rs`」→「`src/cursor/`」。テストは `cursor/tests.rs` へ、「構築のみ」規則は維持 |
| C4 | `ime_indicator::run` の判断を純粋関数に | ★ | 中 | 不要 | `fn decide(sample: &Sample, settings: &IndicatorSettings, last: &Last) -> Verdict { panel: Show / Hide / Keep, cursor: Option<bool> }` を `ime_indicator/decide.rs` に。ADR 0023・0026・0072 の規則がテストになる（シェル面はスキップ、unknown は消さない、同じ対象では再表示しない）。`run` はタイマーと配送だけ |

### D. GUI・表示

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| D1 | `gui/config_window.rs` を画面単位に分割 | ★★★ | 低 | 不要 | `config_window/mod.rs`（`ConfigWindow` / `ui` / `PendingAction` / `apply` / `intercept_close`）、`address_bar.rs`（`header_ui` / `address_row` / `FileList` / `elide_path` / `file_menu_ui` とそのテスト）、`view.rs`（`list_ui` / `breadcrumb_ui` / `general_ui` / `keymap_ui` / `rules_ui` / `ime_ui` / `macro_note_ui` / `recorded_macro_ui`）、`edit.rs`（`EditState` / `Capture` / `Notice` / `start_edit` / `save` / `revert` / `footer_ui` / `edit_body_ui` / `edit_list_ui` / `keymap_edit_ui` / `general_edit_ui` / `notation_cell` / `capture_button` / `color_cell`）、`help.rs`（`notation_help_ui` / `key_names_help`）、`widgets.rs`（`section` / `field` / `note` / `table` / `cell_spacing` / `app_table` / `settings_table`）。`general_edit_ui` は節ごとに分ける。**egui の ID（`salt`）文字列は変えない**（UI テストのセレクタ） |
| D2 | `i18n.rs` を画面単位に分割 | ★★★ | 低 | 不要 | `i18n/mod.rs`（`Lang` / `init` / `t` / `Texts` の定義と EN・JA の値）、`i18n/tray.rs`、`i18n/config_window.rs`、`i18n/log.rs`（`debug_*` / `session_*`）、`i18n/macro_record.rs`、`i18n/cursor.rs`、`i18n/cli.rs`（`help_text`）。**整形関数は「1 関数の中で両言語を `match`」のまま**。`Texts` 120 フィールドは分割しない（型検査による欠落防止がこの構造体の価値） |
| D3 | `gui/mod.rs` の状態を 1 構造体に | ★★ | 中 | 不要 | `gui/state.rs` に `struct AppState { config_path, config_memory, loaded_stamp, status, started_at, wake_context }` を `OnceLock<Mutex<AppState>>` 1 つで持つ。既存のアクセサ（`active_config_path` / `set_status` …）は署名を変えずに残す。テストはプロセス全域の OnceLock に依存しない形に（`AppState` を値で作って検証） |
| D4 | `gui/log.rs:window_ui` の分解 | ★ | 低 | 不要 | `toolbar_ui` / `rows_ui` / `copy_all` に。`draw_row` は既にある |
| D5 | `tray::init` のメニュー構築を関数化 | ★ | 低 | 不要 | `build_menu(texts) -> (Menu, MenuItems)`。**作成順の制約（ADR 0079 影響・補足）はコメントごと残し、ID の実測表を doc コメントに移す** |
| D6 | `theme.rs` | — | — | — | 現状維持（規約 §12 に合致） |

### E. 起動

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| E1 | `main.rs` の段階分割と `cli.rs` | ★★ | 低 | 不要 | `cli.rs`（`CliArgs` / `parse_args` / `extract_lang` / `default_config_path` / `memory_path` とテスト）。`main.rs` の `run` は `choose_config_file(&cli) -> ChosenConfig`、`load_config` 、`go_resident(...)` の段階に。**順序と各段階の理由コメントは残す**。`ensure_config` はそのまま |

### F. テスト

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| F1 | **不変条件テスト** `tests/invariants.rs` | ★★★ | 低 | **要** | (a) `unsafe` の実使用が `tests/unsafe-allowlist.txt` 記載のファイル以外に無い、(b) `unsafe {` / `unsafe fn` の直前（空行・属性を挟まず）に `// SAFETY:` がある、(c) lib 側（`src/lib.rs` の `pub mod` が指すファイル）に `windows::` / `cfg(windows)` が無い、(d) `examples/*.toml` が `Layout::empty()` でもパースできる（`minimal.toml` は既存。他は `a_keyboard()` 前提なので対象を選ぶ）。許可リストのファイルには各行に根拠 ADR を併記し、AGENTS.md 不変条件 3 はこのファイルを参照する形に短縮（H1）。ADR は「不変条件の機械検査」の方針として |
| F2 | clippy `undocumented_unsafe_blocks` | ★ | 低 | オーナー判断 | `Cargo.toml` の `[lints.clippy]` で有効化できるが、guidelines §1 の「設定ファイルによるカスタマイズはしない」に当たる。F1(b) で同じことが検査できるので、**採らない方を推奨**（§8-4） |
| F3 | テスト配置の統一 | ★★★ | 低 | 不要 | A3 と同じ |
| F4 | `cursor` テストの規則維持 | ★★ | — | 不要 | C3 で移すとき、`the_session_has_no_i_beam` のスキップ条件と「インストールしない」をそのまま残す |
| F5 | `tests/ui` の重複調査 | ★ | — | 不要 | 調査結果を `docs/v1.1/notes/` に残すだけ。書き直しは §9 |
| F6 | 受け入れチェックリスト | ★★★ | — | 不要 | §7 |

### G. ビルド・依存・CI

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| G1 | `examples/` の整理 | ★★ | 低 | **要** | 調査プローブ 3 本（`accesskit_probe.rs`・`egui_ime_probe.rs`・`ime_probe.rs`）を削除する（履歴に残る。各 ADR から「当時の検証コードは `git show <sha>:examples/…`」と参照）。`ime_probe.rs` の削除で許可リストが 1 行減る。道具 2 本（`app_icons`・`msix_assets`）は残し、`docs/06_icon-assets.md` から呼ぶ。オーナー判断（§8-5） |
| G2 | eframe フォークの方針 | ★★ | — | **要** | ADR 0074 の後日談として、(a) フォーク維持＋egui リリースごとの rebase 手順を `docs/03_release-operations.md` か新ノートに書く、(b) 子ビューポートの AccessKit を諦め UI テストを別方式にする、のどちらかを決める。**v1.1 では (a) を仮採用し手順だけ書く**。`Cargo.toml` の「REMOVE THIS once upstream」コメントは現状に合わせて書き換える |
| G3 | `Cargo.toml` の注記 | ★ | 低 | 不要 | `winit` の版を eframe と揃える理由を `Cargo.toml` 側にも 1 行 |
| G4 | CI | ★★★ | 低 | 不要 | F1 は `cargo test` に乗るので新ジョブは不要。`cargo doc --no-deps` の警告ゼロをステップに足すか検討（doc コメントのリンク切れが分割で起きやすい） |

### H. ドキュメント

| ID | 施策 | 優先 | リスク | ADR | 内容 |
|---|---|---|---|---|---|
| H1 | unsafe 許可リストの正本を 1 つに | ★★★ | — | F1 と同一 | 正本は `tests/unsafe-allowlist.txt`。AGENTS.md 不変条件 3 は「許可ファイルは `tests/unsafe-allowlist.txt`（根拠 ADR 併記）。各 unsafe に `// SAFETY:` 必須。`tests/invariants.rs` が検査する」に短縮。ブリーフ §5-3 と guidelines §3 は同じ 1 文に |
| H2 | ブリーフ §4・§7 の現行化 | ★★★ | — | 不要 | モジュール表に現行の全モジュール（分割後の姿）を載せる。§7 のリポジトリ構成を `docs/<version>/` 構成・`site-src/`・`packaging/`・`tests/ui/` を含む現行の形に。「最終更新」を更新 |
| H3 | ブリーフと AGENTS.md の正本関係 | ★★ | — | オーナー判断 | 現状「矛盾時はブリーフが正」。提案: **不変条件（ブリーフ §5）と開発規約（§8）の正本を AGENTS.md に移し、ブリーフ §5・§8 は要約＋参照にする**。ブリーフは「経緯と要件の一次資料」に専念する。CODEOWNERS の対象なのでオーナーが直接レビュー（§8-3） |
| H4 | ADR の通し索引 | ★★ | — | 不要 | `docs/decisions-index.md`: 番号・題・フォルダ・ステータス・supersedes / superseded by。手で維持し、`docs/README.md` の ADR 規約に「新規 ADR は索引にも 1 行足す」を追加。0029 ↔ 0068 のような部分上書きはステータス欄に書く |
| H5 | `docs/README.md` の表の修正 | ★★★ | — | 不要 | v0.9 と v1.0 の間の空行を除き、v1.1 の行を足す（**本計画の作成と同時に実施済み**） |
| H6 | 古いコード内コメントの現行化 | ★★★ | 低 | 不要 | `lib.rs`（"Phase 2"）、`keymap/mod.rs` の `tests` 可視性コメント等、分割で場所が変わる参照（`hook.rs:NNN` のような doc 内の行参照は無いが、ファイル名参照は追随させる） |
| H7 | guidelines §5 の分割指針 | ★ | — | 不要 | 「300 行超で構造化」に、本計画で採った分け方（画面単位・読み書き・純粋部と Win32 部）を指針として 3 行追記 |

---

## 5. 進め方（マイルストーン）

各ブランチは `develop` から切り、`--no-ff` でマージし、マージ後に削除する（[04_git-branching.md](../04_git-branching.md)）。コードの整理は `refactor/*`、文書だけなら `docs/*`、版番号などは `chore/*`（§8-2）。コミットは `refactor:` / `test:` / `docs:`。**P7 のとおり、ファイル移動だけのコミットと変更のコミットを分ける**。

| M | 内容 | 施策 | 完了条件 |
|---|---|---|---|
| M0 | 準備 | `Cargo.toml` を 1.1.0 へ（§2.6）、`docs/v1.1/03_acceptance-checklist.md` を v1.0.1 から継承して作成、`04_git-branching.md` と AGENTS.md に `refactor/*` | `develop` のビルドが `WinRemap v1.1.0` を名乗る |
| M1 | **安全網** | F1・H1・A3/F3・A5・H5・H6 | `tests/invariants.rs` が CI で緑。許可リストの正本が 1 つ。コードの移動を始める前に終える |
| M2 | **低リスクの分割**（移動のみ） | D1・D2・C3・A1・A4・E1・B1・B2・D3・D4・D5 | 各ブランチで `cargo fmt` / `clippy -D warnings` / `cargo test` が緑、`tests/ui/run-vm-ui-test.ps1` が v1.0.1 と同じ結果。公開 API（`pub` の署名）と egui の ID 文字列は不変 |
| M3 | **重複の統合** | C1・C2・A2・B4・B6・B5 | 同上に加え、`--debug` ログの同一性（§6 の手順）。unsafe 許可リストの差分（`thread_msg.rs` 追加、`overlay.rs`・`banner.rs` は残る）が ADR と一致 |
| M4 | **フック判断の純粋化** | B3・C4 | engine のテストが緑。受け入れ C（キー変換）・M（IME）の区切りを実キーボードで通す（ADR 0069・0070 のハーネス）。**通らなければ B3 を v1.2 へ送り、v1.1 は M3 までで出す** |
| M5 | **ドキュメント現行化とリリース** | H2・H3・H4・H7・G1・G2・G3・G4 | ブリーフ §4 の表が分割後の `src/` と一致。`release/1.1.0` へ |

順序の理由: M1 を先にするのは、移動の最中に unsafe が許可外へ漏れたり SAFETY コメントが落ちたりしたとき CI が捕まえるため。M2 は「動かすだけ」で、どれも単独でマージできる。M3 は unsafe の場所が変わるので ADR と許可リストが要る。M4 だけがフック経路の実装に触るので最後に置き、受け入れで守る。

---

## 6. リスクと対策

| リスク | 対策 |
|---|---|
| フック経路の挙動が変わる（B3・B4） | M4 に隔離。受け入れの C・M 区切り（実キーボード、注入禁止）。加えて**ログの同一性チェック**: 同じ `tests/acceptance/*.toml` で v1.0.1 と v1.1 を `--debug > log.txt` で起動し、同じ打鍵列（受け入れ C-1〜C-5 の手順）を打って `log.txt` の時刻列を除いた差分がゼロであることを見る |
| 許可リストの変更が AGENTS.md（CODEOWNERS 対象）に波及する | B1・C2・C3・G1 は同じブランチで `tests/unsafe-allowlist.txt` と AGENTS.md を直し、PR でオーナーがレビュー。F1 が CI で両者の一致を強制 |
| 大きな移動コミットがレビューできない | P7。`git log --follow` と rename 検出が効く粒度（1 コミットで 1 ファイルを 1 か所へ） |
| `i18n` 分割で片言語の追加漏れ | 関数内 `match lang()` の形を維持（型検査で網羅が落ちない）。D2 の完了条件に「`grep -c "Lang::En =>"` と `"Lang::Ja =>"` が等しい」を含める |
| egui の ID（`salt`）が変わって UI テストが壊れる | D1 で ID 文字列を定数に切り出し、値は変えない。M2 の完了条件に VM テストを含める |
| `gui/mod.rs` のテストが OnceLock 前提（D3） | `AppState` を値で作る形に直す。プロセス全域の状態を set するテストを残さない |
| eframe フォークの rebase と混ざる | この版では `Cargo.lock` の egui/eframe を動かさない（G2 は方針の文書化のみ） |
| 「ついでに直す」が混ざる | P1。見つけた不具合は `docs/v1.1/notes/` に日付付きで記録し、別ブランチ・別 ADR |

---

## 7. 受け入れ

`docs/v1.1/03_acceptance-checklist.md` は M0 で [v1.0.1](../v1.0.1/03_acceptance-checklist.md) から継承する（C・M・F・H・P の区切りはそのまま）。この版で足す項目:

| 項目 | 通過条件 |
|---|---|
| R-1 ログの同一性 | §6 の手順で、時刻列を除いた `--debug` ログが v1.0.1 と一致する |
| R-2 設定ファイルの書き戻しの同一性 | 設定ウィンドウで「編集 → 何も変えずに保存」した `personal-ja.toml` が 1 バイトも変わらない（`draft` のテストで担保しているが実機でも確かめる） |
| R-3 不変条件テスト | `cargo test --test invariants` が緑。許可リスト外に unsafe を 1 つ書くと赤になる（故意に書いて確かめ、戻す） |
| R-4 UI テスト | `tests/ui/run-vm-ui-test.ps1` の全検査が v1.0.1 と同じ結果 |

---

## 8. オーナー決定（2026-10-29）

計画の提示時に未決としていた 7 点への回答。

| # | 問い | 決定 |
|---|---|---|
| 1 | v1.1 を「中身を整える版」とし、利用者から見える機能を入れないか | **進めながら決める。** この計画の施策は実施する。機能を入れない版にするかは、M5 のリリース判断までに決める。v1.0 計画 §3 の持ち越し 2 件（スタートアップ登録の案内・自前ビルド手順）はドキュメントのみなので、リリースを待たず `develop` → `main` で出せる（04_git-branching §2.5.1） |
| 2 | ブランチ種別に `refactor/*` を足すか | **足す。** `04_git-branching.md` §2.2 と AGENTS.md ワークフロー 4 に追記（M0） |
| 3 | unsafe 許可リストの正本を `tests/unsafe-allowlist.txt` にし、AGENTS.md・ブリーフ・guidelines はそれを参照する形にするか（H1）。不変条件と規約の正本を AGENTS.md に移すか（H3） | **承認。** H1 は M1、H3 は M5 |
| 4 | clippy の `undocumented_unsafe_blocks` を `Cargo.toml` の `[lints]` に置くか（F2） | **置かない**（推奨案）。guidelines §1 を守り、F1(b) で検査する |
| 5 | `examples/` の調査プローブ 3 本を削除するか（G1） | **削除する**（推奨案）。M5 |
| 6 | B3（フック判断の純粋化）を v1.1 に含めるか | **含める。** M4 として実施し、受け入れで確かめる |
| 7 | eframe フォークの方針（G2） | **(a) フォーク維持＋手順の文書化**（推奨案）。M5 |

---

## 9. スコープ外（この版でやらないこと）

- 機能の追加・変更。Non-goals（ブリーフ §3.3）の再検討
- `eframe` / `winit` / `toml` / `toml_edit` / `tray-icon` の入れ替えや大版上げ（`Cargo.lock` は M0 のバージョン更新以外で動かさない）
- ワークスペース分割、crates.io 公開
- `tests/ui/`・`tests/acceptance/` の PowerShell の書き直し（調査のみ、F5）
- `site-src/`、`packaging/`、`installer/`
- 性能改善。フック経路の処理は「変えない」が目的で、速くする試みはしない
