# ADR 0076: 拡大表示では、カーソルを DPI 非対応の文脈で読む

- 作成日: 2026-08-15
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- ステータス: **承認（2026-08-15）**。オーナーの報告（ストア版 0.9.0 で I ビームに色が付かない）を受けた修正
- 関連: [ADR 0067](../../v0.8/decisions/0067-ime-cursor-color.md)（IME をマウスカーソルの色で示す）、[ADR 0073](../../v0.9/decisions/0073-restore-from-a-snapshot-taken-at-startup.md)（復元を起動時の複製から行う）、[ADR 0075](../../v0.9/decisions/0075-a-half-installed-tint-must-say-so.md)（半分だけ着いた着色は、そう言わなければならない）。**本 ADR は、その 3 つが追い続けていた症状の原因を特定する**
- 公式:
  - [`SetThreadDpiAwarenessContext`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext)
  - [`LoadImageW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-loadimagew)
  - [`SystemParametersInfoW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)
  - [High DPI Desktop Application Development on Windows](https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows)

## 背景

**2026-08-15、オーナーの環境で「IME をオンにしても I ビームに色が付かない」**。矢印には付く。環境は Microsoft Store 版 0.9.0（`winremap.exe` は per-monitor DPI aware、実測 `GetProcessDpiAwareness` = 2）、Windows 11 26200、**画面の拡大率 150%**（システム DPI 144、`SM_CXCURSOR` = 48）。

ログには ADR 0075 が入れた理由の行が出ていた。

```
カーソル: I ビームに描かれている画素がないので、着色しませんでした（空のカーソルを元にすると、また空ができます）
11:31:22.736 [IME]   カーソル: 着色した — 差し替え 1・失敗 1
```

**ADR 0073 の決定 4 も、ADR 0075 の報告経路も、正しく働いている。** 分かっていなかったのは、**なぜ元のカーソルが空なのか**である。

### 測ったこと

`LoadImageW(NULL, IDC_IBEAM, IMAGE_CURSOR, 0, 0, LR_SHARED | LR_DEFAULTSIZE)` の戻りを、同じ 1 台・同じ瞬間に、スレッドの DPI 文脈だけ変えて読んだ（2026-08-15、拡大率 150%）。

| 読む側のスレッド | I ビーム | 矢印 |
|---|---|---|
| **DPI 対応**（`winremap.exe` そのもの） | カラー 48×48・アルファ全画素 0・AND マスクも不透明ゼロ → **描画 0 画素** | カラー 48×48・**280 画素** |
| **DPI 非対応**（PowerShell、マニフェストの無い cargo test バイナリ） | **マスク専用 32×32・26 画素** | カラー 32×32・144 画素 |

**サイズ引数は効かない。** `0,0` / `32,32` / `48,48` のいずれで頼んでも、DPI 対応スレッドには同じ空の 48×48 が返る（同日実測）。`LR_SHARED` はそのスレッドの DPI 文脈に対して用意された 1 つを返すためで、**文脈だけが唯一のレバー**である。

### なぜ I ビームだけなのか

素の I ビームは、[`from_mask_only`](../../../src/cursor.rs) が説明しているとおり **AND=1/XOR=1 で画面を反転させて描くマスク専用カーソル**である。反転で描かれた画素は「載せる色」を持たないので、**32bpp のカラービットマップへ変換すると何も残らない**。矢印はアルファ付きのカラーカーソルなので、拡大しても中身がある。**着色が半分だけ効いていた理由がこれである。**

### なぜ 3 つの版にわたって捕まらなかったのか

**この不具合を計測しうる場所が、すべて実質 DPI 非対応だった。**

- `tests/acceptance/probe-ime-cursor.ps1` は PowerShell で走る
- 単体テストの実行ファイルは `winremap.exe` のマニフェストを持たない
- CI（`windows-latest`）は拡大率 100%

そして [ADR 0073](../../v0.9/decisions/0073-restore-from-a-snapshot-taken-at-startup.md) が「`SPI_SETCURSORS` は前の実行が残した着色を消すと実測した（2026-08-09）」と書いたときの実測も、**プローブから**である。**WinRemap から呼ぶと逆のことが起きる**（次項）。

v0.8 の M-2 が「間欠的」に見えたのも、これで説明が付く。per-monitor DPI なので、**どの拡大率の文脈で読んだかによって結果が変わる**。

### `SPI_SETCURSORS` 自身も、同じ理由で壊す

同日の実測（セッションに登録されているカーソルを、毎回別プロセスから読む）。

```
after SPI_SETCURSORS from DPI-UNAWARE  ibeam: 0 2048 2048 0 0    ← 素のマスク専用
after SPI_SETCURSORS from DPI-AWARE    ibeam: 0 1024 1024 1024 0 ← 空
```

**DPI 対応スレッドから呼ぶと、空の I ビームがセッションのカーソル表に入る。** そのあと `capture_pristine` の `snapshot` は「描かれている画素が無い」として複製を採らない（`NoSnapshot`）。つまり**復元のために置いた手順が、復元の手段そのものを奪っていた** — ADR 0073 決定 1 の第 1 段は、拡大表示の環境では最初から死んでいたことになる。

## 決定

### 決定 1 — 空だったときに限り、DPI 非対応の文脈で**やり直す**

`unscaled_retry` が、読み取りから作成までを 1 単位として扱う。1 回目は呼び出し元の文脈のまま走り、**「カーソルが空だった」ときだけ** `Unscaled`（`SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE)` の RAII ガード）の下でもう一度走る。それ以外の失敗は、そのまま呼び出し元の答えになる。

**読み取りだけを囲んでも効かない。** 共有カーソルのハンドルは、**それを使う側**のスレッドの DPI 文脈に対して解決される。非対応の文脈で採ったハンドルを対応の文脈で `GetIconInfo` すると、また空が返る。この修正の最初の版は `LoadImageW` だけを囲んでいて、**何も変わらなかった**（実測して分かった）。やり直す単位は「読んで、作るところまで」でなければならない。

### 決定 2 — `SPI_SETCURSORS` は常に DPI 非対応で呼ぶ

こちらは条件を付けない。この呼び出しが影響するのは自分のプロセスだけではなく、**セッションのカーソル表**だからである。

### 決定 3 — 回帰テストは単体テストに置く

`a_dpi_aware_app_can_still_tint_the_i_beam` と `a_dpi_aware_app_can_still_snapshot_the_i_beam` は、スレッドを per-monitor DPI aware にしてから `tinted` / `snapshot` を呼ぶ。**作るだけで、設置はしない**ので、ADR 0073 決定 6 の「カーソルを差し替える検査はプローブの仕事」を破らない。

**この 2 件は拡大表示の画面でしか意味を持たない。** 100% では拡大形が無いので、修正の有無にかかわらず通る。それでも置くのは、**開発機が 150% であり、そこで最初に落ちるから**である。プローブ（PowerShell）はこの不具合を構造的に踏めないので、代わりにはならない。

## 却下した案

- **常に DPI 非対応で読む** — 1 行で済むが、**矢印が 32×32 になる**。150% の画面では拡大されたぼやけた矢印になり、「利用者が実際に持っているカーソルを、色だけ変えて使う」（ADR 0067）が崩れる。**見えるものを下げて、実装の単純さを買う**ことになる
- **サイズを明示して読む** — 効かない。`0,0` / `32,32` / `48,48` のどれでも同じ空の 48×48 が返る（実測）
- **I ビームだけを特別扱いする**（マスク専用カーソルの一覧を持つ） — **Windows がそのカーソルの作りを変えた瞬間に嘘になる**一覧を抱えることになる。「空だったらやり直す」は一覧を要らなくし、将来 `REPLACED` に足すカーソルにもそのまま効く
- **プロセス全体を DPI 非対応にする** — 設定ウィンドウとログウィンドウがぼやける。**カーソルのために GUI を犠牲にする**話になる
- **32×32 を読んで、自前で 48×48 へ拡大する** — 拡大処理を抱えることになる。素の I ビームはセッションのカーソル表にも 32×32 で登録されているので、**置き換えとしてサイズは揃っている**（Windows が描画時に拡大するのは素のものと同じ扱いである）

## 影響・補足

- **実害の範囲（当初の見立てを訂正）。** 登録されるカーソルは空になっていたが、`SPI_SETCURSORS` は各アプリにカーソルの読み直しを促すため、**各アプリは自分の DPI 文脈で正しい I ビームを持ち直す**。オーナーの観察どおり、画面に描かれる I ビームは正常で、症状は「色が付かないだけ」だった。**`change_cursor_color` を使っていない利用者への実害は無い**
- **v0.8 の M-2（何も描かれない I ビーム）と、v0.9 以降の「着色が入らない」は、同じ 1 つの原因の 2 つの見え方である。** ADR 0073 以前は、この空を元に着色して**設置していた** — `SetSystemCursor` は共有オブジェクトそのものを差し替えるので、そちらは全アプリの描画に出た。0073 が設置を止めた結果、0.9.0 では「着かない」形になった
- **受け入れの M-2 は、拡大表示の画面で測ること。** 100% の画面では、修正前のバイナリでも通ってしまう
- **プローブの復元側の検査は、いまも矢印しか測っていない**（`startup-clears-any-leftover`・`restores-when-the-ime-goes-off`・`the-next-start-restores-it`、および繰り返し検査のオフ側）。I ビームを測るのは IME がオンの間だけである。本 ADR の範囲外だが、**「復元したあと I ビームが空で残る」は現在の 10 項目を全部通過する**
