# ADR 0086: フックが 1 つのキーイベントで何をするかは、ライブラリの純粋な状態機械が決める

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- ステータス: **承認（[v1.1 開発計画](../01_development-plan.md) B3、オーナー決定 2026-10-29「B3 も v1.1 に含める」）**
- 関連: [ADR 0003](../../v0.1/decisions/0003-config-reload-arc-swap.md)（表の atomic swap）、[ADR 0004](../../v0.1/decisions/0004-keymap-matching-priority.md)（マッチングの優先順位）、[ADR 0005](../../v0.1/decisions/0005-modifier-lift-restore.md)（修飾キーのリフトと復元）、[ADR 0006](../../v0.1/decisions/0006-injection-markers.md)（注入マーカー）、[ADR 0013](../../v0.1/decisions/0013-two-stroke-sequences.md)（2 ストローク）、[ADR 0015](../../v0.1/decisions/0015-menu-mask-key.md)（メニューガード）、[ADR 0016](../../v0.1/decisions/0016-debug-key-logging.md)（デバッグログ）、[ADR 0043](../../v0.3/decisions/0043-runtime-macro-recording.md)・[ADR 0044](../../v0.3/decisions/0044-macro-replay-thread.md)（マクロ記憶）、[ADR 0053](../../v0.4/decisions/0053-test-inject-mode.md)（`--accept-injected`）、[ADR 0084](0084-split-unsafe-modules-by-file.md)（`hook/` の分割）

## 背景

ブリーフ §4 の原則は「**フック層は薄く、ロジック層は厚く**」である。v0.1 ではそのとおりだった — `hook.rs` はイベントを `keymap::resolve` に渡し、答えを `sender` に流すだけだった。

v1.0.1 までに、フックの中で決めることが増えた。

| 決めること | 入った版 | ADR |
|---|---|---|
| 修飾キーの論理状態（注入した置換出力も数える） | v0.1 | 0005・0006 |
| 押下中のキーの扱い（リピート中に修飾キーが変わっても出力を変えない、離したら何を出すか） | v0.1 | 0005 |
| 2 ストロークの待ちとスワロー | v0.1 | 0013 |
| Alt/Win のメニューガード | v0.1 | 0015 |
| デバッグ行の種類と、リピートを省く規則 | v0.1〜v0.5 | 0016・0057 |
| 記憶キーの優先と、出力を記録する規則 | v0.3 | 0043 |
| 注入イベントの扱いと `--accept-injected` | v0.4 | 0053 |

これらは `hook.rs` の `on_key_down`（136 行）・`handle_event`（104 行）・`on_key_up` に、7 つの `thread_local` と `sender::*` の呼び出しと絡み合って書かれていた。**どれにも単体テストが無い。** フック層はテスト免除（ブリーフ §9）だが、それは「ロジックを持たせない」と対で成り立つ約束で、いちばん厚いロジックがフックにあった。2 ストロークやメニューガードの振る舞いを確かめる手段は、受け入れで実キーボードを打つことだけだった。

## 決定

### 決定 1 — `winremap::engine::Engine` が決め、`hook` は訳して従う

ライブラリに `engine` モジュールを置く。

```text
hook::handle_event(KBDLLHOOKSTRUCT)
  │  KeyEvent { vk, down, injected: Option<Injected> }   ← dwExtraInfo のマーカーを読む
  │  Options  { enabled, accept_injected }               ← トレイのトグルと ADR 0053
  │  table: Option<&RemapTable>, exe: &str               ← ArcSwap の load と前面キャッシュ
  ▼
Engine::on_event(..) -> Decision
  │  suppress   : コールバックが非 0 を返すか
  │  emit       : sender に流す 1 件（ExactDown / ExactRepeat / ExactUp / KeyOnly / MaskedModifierUp / Sequence）
  │  ime_poke   : IME インジケーターに渡す chord
  │  debug[2]   : デバッグ行（物理イベント、判定）
  │  records[9] : 記憶イベント（マクロ 1 要素につき 1 件、または記憶キーの遷移）
  ▼
hook: ログのリングへ・IME へ・記憶のリングへ・sender へ
```

`Engine` が持つ状態は、これまで `thread_local` に散っていたもの — `sides`（修飾キーの論理状態）、`active[256]`（押下中のキーの扱い）、`pending`（2 ストロークの 1 打目）、`menu_guard`、`phys_down[256]`、`recorder` — で、`hook` には `ENGINE: RefCell<Engine>` 1 つとデバッグ・記憶の 2 つのリングだけが残る。

### 決定 2 — 不変条件 2 の性質は `Decision` の形で担保する

コールバックの中で動くので、ヒープ確保もロックも禁止のまま（AGENTS.md 不変条件 2）。

- `Engine` の状態は固定長配列と `Copy` 型。`Engine::new` は `const fn` で、`thread_local` の `const` 初期化に乗る
- `Decision` は `Copy` で、デバッグ行は最大 2 件、記憶イベントは最大 9 件（マクロの上限 8 ＋ 1）の固定長配列
- マクロの要素列は表から**借りる**（`Emit::Sequence(&[KeyCombo])`）。表は `ArcSwap` の guard が生きている間だけ借りられ、`handle_event` の中で使い切る
- 記憶の「再生できるか」（スレッドが居るか、録れているか、再生中か）はマクロ記憶スレッド側の状態なので、`Engine` は `RecordNote::Play { held }` を返すだけにし、`hook` が `request_replay` を呼んで断られた理由（`Ignored` / `NothingToPlay`）に訳す

### 決定 3 — 送出の修飾キー手術は、判断側で 1 回だけ計算する

`send_exact_down` は `held` から `lifted` / `added` を自分で計算していた。`Engine` が押下時に `exact_adjustment(target.mods, held)`（`keymap::sides`、B4）で求めた `ModAdjustment` を `Emit::ExactDown` に載せ、`sender` はそれを**送るだけ**にする。離すときに戻す量と、押すときに動かした量が、同じ 1 つの値になる。

### 決定 4 — 適用の順序は従来のコールバックと同じ

デバッグ行 → IME の poke → 記憶イベント → `SendInput`。どれもリングへの書き込みか 1 回の `PostThreadMessageW` で、`SendInput` が最後なのは従来どおり。

## 理由

- **中核の振る舞いがテストになる。** `engine/tests.rs` は 2 ストロークの成立と不成立、リピート中の修飾キー変化、メニューガード、注入イベントの素通しと `--accept-injected`、無効化中のリピート継続、記憶キーの優先と記録される内容、IME の poke を、キーイベントの列として確かめる。ヘッドレス CI で回る
- **フックが本当に薄くなる。** `hook/mod.rs` の `handle_event` は訳して・決めてもらって・従う 60 行で、`sender` を呼ぶ `apply` が 1 か所。ブリーフ §4 の図のとおりになる
- **性質は変えない。** 固定長・借用・`Copy` で、コールバック内の処理は従来と同じ量である。判断そのものは移しただけで、分岐は 1 つも足していない

## 却下した代替案

- **フックにテストを書く（`thread_local` を注入可能にする）** — `sender` の呼び出しを差し替える仕組みが要り、テストのためだけの間接層がフック経路に入る。決める側を値にする方が、間接層が無い
- **`Decision` を `Vec` で返し、フックで実行する** — コールバック内の確保になる（不変条件 2）。固定長で足りる上限が全部分かっている（デバッグ 2・記憶 9・送出 1）
- **`Engine` を `hook` モジュール内に置く（バイナリ側のまま）** — テストはバイナリ側でも CI で回るが、`windows` クレートに依存しない純粋ロジックはライブラリの側に置く規約（guidelines §5）で、`keymap` や `recorder` と同じ場所が正しい
- **`sender` に `ModAdjustment` の計算を残し、`Engine` も同じ計算をする** — 2 か所が同じ答えを出すことを祈る形になる。決定 3 のとおり 1 回にする

## 影響・補足

- **受け入れ H-1・H-2・H-7 を必ず測る**（[03_acceptance-checklist.md](../03_acceptance-checklist.md)）。R-1 の `--debug` ログ比較で、判定行・入力行・注入行が v1.0.1 と一致することも見る
- `DebugAction` / `DebugEvent` / `Injected` はライブラリの型になった（`hook/debug.rs` は文言を付けるだけ）
- `sender::send_exact_down(target, adjustment)` に署名が変わった。呼び出し側は `hook::apply` の 1 か所
- IME インジケーターの `notify_keydown` は、従来はフックが「下向きの物理イベントすべて」で呼んでいた。`Decision::ime_poke` も同じ条件（注入でない down）で `Some` になる。修飾キーかどうかの判断はこれまでどおりインジケーター側
