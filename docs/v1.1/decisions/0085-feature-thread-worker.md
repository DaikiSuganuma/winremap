# ADR 0085: 機能スレッドの起動・停止とスレッドメッセージの配管を 1 か所ずつにする

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- ステータス: **承認（[v1.1 開発計画](../01_development-plan.md) C1・C2、オーナー承認 2026-10-29）**
- 関連: [ADR 0020](../../v0.1/decisions/0020-ime-indicator-scope.md)（IME インジケーターのスレッド）、[ADR 0044](../../v0.3/decisions/0044-macro-replay-thread.md)（マクロ記憶のスレッド）、[ADR 0083](0083-invariants-as-tests.md)・[ADR 0084](0084-split-unsafe-modules-by-file.md)（許可リストと分割の規則）

## 背景

IME インジケーター（`src/ime_indicator/`）とマクロ記憶（`src/macro_record/`）は、どちらも「設定で有効になったら専用スレッドを 1 本起こし、そのスレッドがウィンドウを持ってメッセージループを回す。フックからは有界な `PostThreadMessageW` 1 回で合図する」という同じ形をしている（ブリーフ §5-2 の例外 3・4）。

同じ形なので、コードも同じになった。

| 重複 | `ime_indicator/mod.rs` | `macro_record/mod.rs` |
|---|---|---|
| スレッドの起動（ready チャネルで tid を受け取り、`THREAD_ID` / `THREAD_HANDLE` に置き、死んだ旧スレッドを join） | `start_thread` 151–178 行 | `start_thread` 512–539 行 |
| スレッド本体（資源を作り、失敗なら報告して終わり、成功なら tid を送って `catch_unwind` で本体を回す） | `thread_main` 180–196 行 | `thread_main` 541–557 行 |
| 停止（tid を 0 にして `WM_QUIT` を投げ、join） | `stop` 138–149 行 | `stop` 500–510 行 |

| 重複 | `ime_indicator/overlay.rs` | `macro_record/banner.rs` |
|---|---|---|
| `current_thread_id` / `post_to_thread` / `post_quit_to` / `next_thread_message` | 384–431 行（タイマー 2 関数を含む） | 325–360 行 |

後者は unsafe の重複でもある。v1.0.1 時点で `overlay.rs` 19 ブロック、`banner.rs` 19 ブロックのうち、5 と 7 がこの配管である。

一方、**ウィンドウそのもの（見た目・寸法・描画・タイマーの使い方）は統合しない**。マクロ記憶設計書 §6.1 が決めたとおり、片方は保持してフェードアウトし、もう片方は記録中ずっと出ている。似ているのは「窓を持つスレッド」という殻であって、窓ではない。

## 決定

### 決定 1 — `src/worker.rs` に `Worker` を置く（unsafe なし）

```rust
static INDICATOR: Worker = Worker::new();
INDICATOR.start("ime-indicator", Overlay::create, run, report);
INDICATOR.thread_id();   // フックから wait-free に読む。0 なら誰もいない
INDICATOR.stop();
```

- `start` は**資源の構築をスレッド側で**行い、失敗はそのスレッドが報告して終わる。成功したら tid を公開し、本体を `catch_unwind` で回す。パニックは報告し、tid を 0 に戻す
- `report` は `fn(&str)` で、各機能が自分の i18n 文言（`ime_indicator_failed` / `macro_record_failed`）に包んで通知する。英語の診断文字列（guidelines §11）
- `stop` は tid を 0 にして `WM_QUIT` を投げ、join する。自然死したスレッドは次の `start` が join する

### 決定 2 — `src/thread_msg.rs` にスレッドメッセージの配管を置く（unsafe あり、許可リストに 1 行）

`current_thread_id` / `post_to_thread(tid, message, wparam)` / `post_quit_to` / `set_thread_timer` / `kill_thread_timer` / `next_thread_message`。`overlay.rs` と `banner.rs` からは消える。`post_to_thread` の引数は 3 つに揃える（インジケーター側は `wparam = 0`）。

### 決定 3 — 窓は触らない

`Overlay` / `Banner` の構造体、`wndproc`、`paint`、タイマーの使い分けはそのまま。`overlay.rs` / `banner.rs` に残る unsafe は窓の分だけになる。

## 理由

- **同じ理由で変わるものが 1 か所になる。** 「スレッドが死んだときの扱い」「tid の公開のタイミング」「停止の順序」を変えるとき、2 か所を同じに直す必要が無くなる。v0.3 でマクロ記憶を足したときはインジケーターを写した — 次の機能も写すことになっていた
- **フック側の契約が読める。** 「tid が 0 なら投げない、0 でなければ 1 回だけ投げる」は `Worker::thread_id` の doc 1 か所に書かれる
- **unsafe が減る。** 配管の 12 ブロックが 6 ブロックになり、残りは窓のもの。許可リストの `overlay.rs` / `banner.rs` の行は「窓」とだけ言えばよくなる

## 却下した代替案

- **窓まで含めて 1 つの「オーバーレイ基盤」にする** — 設計書 §6.1 の判断を覆すことになる。2 つの窓は保持の意味が違い、共通化すると片方の都合が他方の制約になる
- **スレッドの起動だけ共通化し、配管は各ファイルに残す** — 配管こそ 1 文字も違わない重複で、unsafe の重複でもある。残す理由が無い
- **`std::thread::scope` 等で構造化する** — 機能スレッドはプロセスの寿命いっぱい生きるか、設定の再読み込みで止まる。スコープに収まらない

## 影響・補足

- `tests/unsafe-allowlist.txt`: `src/thread_msg.rs` を追加（この ADR）。`overlay.rs` / `banner.rs` は残る（窓の unsafe）
- パニック時の報告文は `"{name} thread panicked"` に揃う（旧: `"indicator thread panicked"` / `"macro-record thread panicked"`）。英語の診断文字列で、通知ダイアログにしか出ない
- マクロ記憶の `REPLAYING` を本体終了後に下ろす処理は、`run` の中で drop ガードとして行う（パニックでも下りる — 旧コードと同じ）
