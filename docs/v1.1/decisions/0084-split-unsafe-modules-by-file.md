# ADR 0084: unsafe を含む大きなモジュールはディレクトリ化し、許可リストには unsafe を実際に含むファイルだけを載せる

- 作成日: 2026-10-29
- 作成: Claude Code（AI モデル: claude-fable-5-1）／レビュー・承認: オーナー
- ステータス: **承認（[v1.1 開発計画](../01_development-plan.md) C3・B1、オーナー承認 2026-10-29）**
- 関連: [ADR 0083](0083-invariants-as-tests.md)（許可リストの正本とテスト）、[ADR 0009](../../v0.1/decisions/0009-bootstrap-unsafe-placement.md)（unsafe を置く場所）、[ADR 0067](../../v0.8/decisions/0067-ime-cursor-color.md)・[ADR 0073](../../v0.9/decisions/0073-restore-from-a-snapshot-taken-at-startup.md)・[ADR 0076](../../v1.0/decisions/0076-read-cursors-unscaled.md)（`cursor.rs` の中身）

## 背景

[02_rust-guidelines.md §5](../../02_rust-guidelines.md) は「1 ファイル 300 行超で構造化する」と言い、§3 は「unsafe ブロックは最小スコープ、安全なラッパーで包んで公開する」と言う。この 2 つは、unsafe を含むモジュールが大きくなったときに衝突する。

- `src/cursor.rs` は 1,145 行、unsafe 24 ブロック。中身は 4 種類に分かれる — 公開 API と `Trouble` の報告（ADR 0073）、DPI 文脈の切り替え（ADR 0076）、カーソルの読み出しと着色の組み立て、GDI のビットマップ操作。加えて `outlined` / `and_mask` / `tint` は **Win32 を 1 つも呼ばない純粋なピクセル演算**で、テストもある
- `src/hook.rs` は 905 行、unsafe 15 ブロック。コールバックと状態機械のほかに、デバッグイベントの整形、記憶イベントの配送、単一インスタンス、メッセージループが同居している。unsafe を含むのはそのうち 3 つだけである

ADR 0009 の時点では「unsafe を書くファイルを 3 つに閉じる」ことが分かりやすさだった。ファイルが千行を超えると、**どの行が unsafe なのかを探す方が難しくなる**。

## 決定

### 決定 1 — ディレクトリ化し、責務ごとにファイルを分ける

`src/cursor.rs` → `src/cursor/`、`src/hook.rs` → `src/hook/`。分け方の基準は「Win32 を呼ぶか」と「何の責務か」の 2 軸で、**純粋な演算・整形・配送は unsafe を含まないファイルに置く**。

| `src/cursor/` | 中身 | unsafe |
|---|---|---|
| `mod.rs` | 公開 API（`capture_pristine` / `restore` / `apply` / `install_crash_restore`）、`Trouble` / `Action` / `Kind`、セッション状態の置換と復元 | あり（`SetSystemCursor` / `CopyIcon` / `SPI_SETCURSORS` / 例外フィルター） |
| `scale.rs` | `Unscaled`（DPI 文脈）と `unscaled_retry`（ADR 0076） | あり |
| `tint.rs` | 読み出し（`load` / `snapshot`）、着色の組み立て（`tinted`）、`decode` / `drawn_pixels` | あり |
| `gdi.rs` | ビットマップの読み書き（`read_as_bgra` / `dib_section` / `from_color` / `from_mask_only`）、`Bitmap` | あり |
| `pixels.rs` | `outlined` / `and_mask` / `tint`（純粋） | **なし** |
| `tests.rs` | テスト。「構築するだけで、インストールはしない」規則は変えない（ADR 0073 決定 6） | — |

`src/hook/` は B1 の実施時に同じ基準で分ける（`mod.rs` / `debug.rs` / `record.rs` / `instance.rs` / `message_loop.rs`）。

### 決定 2 — 許可リストには、unsafe を実際に含むファイルだけを載せる

「`src/cursor/` 配下すべて」のようなディレクトリ単位の許可はしない。`pixels.rs` や `hook/debug.rs` が許可外であることは、[ADR 0083](0083-invariants-as-tests.md) のテストが「許可リストにあるのに unsafe が無い」「許可外に unsafe がある」の両方向で守る。

### 決定 3 — 分割は移動だけで行い、関数の中身は変えない

分割のコミットは「ファイルを動かしただけ」にし、可視性（`pub(super)`）と `use` の追加だけを許す。振る舞いの差分はゼロで、受け入れは M-1・M-2・M-4（カーソル）と H-1・H-2・H-7（フック）で確かめる。

## 理由

- **unsafe の位置が読める。** `cursor/` の 24 ブロックは 4 ファイルに散るのではなく「役割の違う 4 か所」に集まる。純粋な演算は unsafe の無いファイルで、そこに unsafe が増えればテストが止める
- **ライブラリへ出す道が開く。** `pixels.rs` は `windows` に依存しないので、将来 `winremap::pixels` へ移してヘッドレス CI で回せる（この ADR では移さない — 使うのが `cursor` だけのうちは、近くにある方が読みやすい）
- **許可リストが「事実の一覧」のまま保てる。** ディレクトリ単位にすると、リストは実態より広くなり、ADR 0083 が消した「意味のない名簿」に戻る

## 却下した代替案

- **1 ファイルのまま `// ---- section ----` で区切る** — 1,145 行の中の区切り線は、`cursor.rs` 自身がすでに使っていて、それでも読めなかった
- **ディレクトリ単位で許可する（`src/cursor/**`）** — 決定 2 のとおり。許可の範囲を広げるのは一覧を短くするためであり、一覧を短くする理由は無い
- **純粋な演算を今すぐ `winremap::pixels` へ移す** — 使い手が 1 つしか無い。ライブラリの公開面を増やす理由が出てから

## 影響・補足

- `tests/unsafe-allowlist.txt`: `src/cursor.rs` の行を `src/cursor/mod.rs` / `scale.rs` / `tint.rs` / `gdi.rs` の 4 行に置き換える（ADR 0067・0076）。`hook.rs` は B1 の実施時に同様
- AGENTS.md 不変条件 3 はファイル名を列挙していないので、変更は無い
