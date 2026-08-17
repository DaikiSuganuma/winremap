# ADR 0080: トレイのアイコンは、通知領域が描くサイズを自分で指定して読む

- 作成日: 2026-08-17
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- ステータス: **承認（オーナー指示 2026-08-17「アイコンが青い塗りつぶしになっています」→ 調査のうえ「2 を進めてください」）**
- 関連: [ADR 0007](../../v0.1/decisions/0007-tray-crate-tray-icon.md)（トレイは `tray-icon` クレート）、[ADR 0010](../../v0.1/decisions/0010-icon-embedding.md)（アイコンは exe に埋め込む）、[ADR 0038](../../v0.2/decisions/0038-gui-win32-module.md)（`gui/win32.rs` とサイズ別のウィンドウアイコン）、[ADR 0025](../../v0.1/decisions/0025-display-name-winremap.md)（製品名の表記）

## 背景

タスクトレイと「設定 > 個人用設定 > タスクバー > その他のシステム トレイ アイコン」で、WinRemap のアイコンが**キーの見えない青い塗りつぶし**になっていた。

素材ではない。切り分けの結果は次のとおり:

- MSIX 版 exe に埋め込まれたアイコン資源そのものは正常（青いキーボードの絵が取り出せる）
- `assets/kbd.ico` は 16 / 24 / 32 / 48 / 256 px の 5 面すべてを 32bpp で持っている
- MSIX の `Square44x44Logo` 系も `BackgroundColor` も正常。そもそも一覧の表示名が小文字の `winremap`（= exe のバージョンリソース由来）だったことから、シェルは**パッケージのロゴではなく exe 側**を見ていると判る

原因は**渡しているアイコンのサイズ**だった。`tray.rs` は `tray_icon::Icon::from_resource(ordinal, None)` を呼んでいる。このクレートの実装は `None` を「幅・高さ 0 ＋ `LR_DEFAULTSIZE`」に落とす:

```rust
// tray-icon 0.24.1 src/platform_impl/windows/icon.rs
let (width, height) = size.unwrap_or((0, 0));
LoadImageW(instance, name, IMAGE_ICON, width, height, LR_DEFAULTSIZE)
```

`LR_DEFAULTSIZE` が意味するのは**大アイコンの寸法**（`SM_CXICON`）であって、通知領域が使う小アイコン（`SM_CXSMICON`）ではない。開発機（表示スケール 150%）で実測した値:

| | 寸法 |
|---|---|
| `SM_CXSMICON`（小） | 24 × 24 |
| `SM_CXICON`（大） | 48 × 48 |
| **修正前に渡していた面** | **48 × 48** |
| 通知領域が描く枠 | 24 × 24 |

つまり **48px の面をシェルに渡し、シェルが 24px へ 2 倍縮小していた**。キーボードの絵はキーとキーの隙間が細く、この縮小で隙間が潰れて青一色の塊になっていた。

これは [ADR 0038](../../v0.2/decisions/0038-gui-win32-module.md) で直したウィンドウアイコンの不具合の**鏡像**である。あちらは winit が小さい 1 枚を大アイコン枠にも入れて引き伸ばしていた。こちらは大きい 1 枚を小アイコン枠に入れて潰していた。同じ「サイズ別の面を持つ `.ico` を、サイズを指定せずに読んだ」という誤りである。

## 決定

### 決定 1 — `SM_CXSMICON` / `SM_CYSMICON` を明示して読む

`gui/win32.rs` に `load_notification_icon(ordinal) -> Option<isize>` を足し、`LoadImageW` に小アイコンの寸法を渡す。得た `HICON` は `Icon::from_handle` でクレートへ渡す。

実測で、修正後は 24 × 24 が返る。これは `.ico` に実在する面なので、**シェル側の拡縮が一切起きない**。

### 決定 2 — 置き場所は `gui/win32.rs`。新しい unsafe モジュールは作らない

トレイは GUI ではないが、必要なのは [ADR 0038](../../v0.2/decisions/0038-gui-win32-module.md) がすでに持っている「**このサイズで描かれる面を読む**」という呼び出しそのものである。専用モジュールを新設すると、関数 1 つのために AGENTS.md 不変条件 3 の unsafe 許可リストが 1 行増える。`gui/mod.rs` から再エクスポートし、`tray.rs` は unsafe を持たないまま（ADR 0007 の前提）にする。

### 決定 3 — `LR_SHARED` を**付けない**

同じファイルの既存の `load_icon`（ウィンドウ用）は `LR_SHARED` を付けている。共有ハンドルはシステムの持ち物で、ウィンドウがこの呼び出しより長生きするからそれでよい。

トレイ用は逆で、**付けてはならない**。`tray_icon::Icon::from_handle` は受け取ったハンドルを `RaiiIcon` に包み、Drop で `DestroyIcon` する。共有ハンドルを `DestroyIcon` することは許されていない。

### 決定 4 — `from_resource` はフォールバックとして残す

資源が読めなかったときは従来の呼び方に落ちる。**絵が 1 段汚いことと、トレイアイコンが存在しないことは、深刻さが違う**。常駐アプリはトレイから終了させるので、アイコンが消えるとメニューへの唯一の入口が消える。

## 理由

- **`.ico` に実在する面を要求するので、拡縮が起きない。** 16 / 24 / 32 / 48 のどれを要求しても素の面が返る
- **DPI に追随する。** 固定 16 を書くと 150% / 200% の環境でぼやける
- **既存の前例に乗る。** 呼び方（`GetSystemMetrics` ＋ `LoadImageW`）は ADR 0038 と同じ。`LoadIconMetric` を使う手もあるが、`Win32_UI_Controls` フィーチャーの追加が要るうえ、DPI の根拠は同じ（プロセスの DPI 認識）

## 却下した代替案

- **`Some((16, 16))` と固定で書く** — 開発機の実測が 24 である以上、明確に誤り。高 DPI でぼやける
- **`LoadIconMetric(LIM_SMALL)` を使う** — Microsoft が通知領域向けに推奨している API で筋は良いが、`windows` クレートに `Win32_UI_Controls` を足すことになる。得られる寸法の根拠は `SM_CXSMICON` と同じなので、依存を増やす見返りが無い
- **16px の絵を作り直して縮小に耐えるようにする** — 症状ではなく原因を直すべき。そもそも 16px の面は**一度も使われていなかった**（要求されていたのは 48px）
- **`tray-icon` に `from_resource(_, None)` の意味を変える PR を出す** — 上流の破壊的変更になる。こちらは `Some(size)` を渡せば済む
- **新しく `src/icon.rs` を作って GUI とトレイで共有する** — 設計としては素直だが、unsafe 許可リストが増える。決定 2 のとおり

## 影響・補足

- **プロセスの DPI 認識に依存する。** `GetSystemMetrics` はプロセスの DPI コンテキストに従う。マルチモニターで倍率が異なる場合、タスクバーのある側と食い違いうる。ただし `.ico` は 16 / 24 / 32 / 48 を持つので、どれが選ばれても実在する面であり、失敗の幅は限られる。`LoadIconMetric` に替えてもこの点は同じ
- **既存のレジストリ行は直らない。** 「その他のシステム トレイ アイコン」の行は `HKCU\Control Panel\NotifyIconSettings` に exe パスごとに残る。**この修正が効くのは、新しいビルドを起動して新しい行ができたとき**
- **同じ画面の表示名も別途直した**（`build.rs` のバージョンリソース）。シェルは表示名を `FileDescription` → `ProductName` / `OriginalFilename` の順に読むが、winresource は前 2 つを crate 名で埋めるため小文字の `winremap` が出ていた。[ADR 0025](../../v0.1/decisions/0025-display-name-winremap.md) に従い、表示テキストは `WinRemap`、ファイル名を指す `OriginalFilename` は小文字のままにしてある。ADR を分けていないのは、これが ADR 0025 の適用であって新しい判断ではないため
- **受け入れ（v1.0 のチェックリストに入れる項目）**: ①トレイのアイコンにキーの隙間が見える ②無効にしたとき、灰色の面でも同じく隙間が見える ③設定 > 個人用設定 > タスクバー > その他のシステム トレイ アイコンで、行の名前が `WinRemap`（大文字の R）になっている
