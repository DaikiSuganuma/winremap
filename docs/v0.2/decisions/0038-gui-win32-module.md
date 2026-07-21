# ADR 0038: GUI 用の Win32 モジュールを unsafe 許可リストに追加する

- ステータス: accepted
- 日付: 2026-07-21
- 作成: Claude Code（AI モデル: claude-opus-4-8）／レビュー・承認: オーナー（2026-07-21）

## 文脈

マイルストーン B0 の実機確認で、egui / eframe の API では解決できない不具合が 2 件出た。

### 1. ウィンドウアイコンが崩れる

> タスクマネージャーのアイコンが壊れたような表示になっています。親プロセスのアイコンは
> 正常に表示されています。

原因は winit の実装にある。egui-winit は `ViewportBuilder::with_icon` の画像を winit の
`Window::set_window_icon` に渡すが、winit のこの関数は **`ICON_SMALL` の枠にしか設定せず、
`ICON_BIG` を空のまま**にする（`platform_impl/windows/window.rs`）。`ICON_BIG` を設定する
`set_taskbar_icon` は Windows 固有の拡張 API で、egui も eframe も公開していない。

結果、Windows は小さいアイコン 1 枚をあらゆるサイズへ引き伸ばして表示する。48px → 32px →
と PNG のサイズを変えて試したが、どのサイズでも「ある場面では潰れる」状態は解消しなかった。
プロセス行のアイコンが正常なのは、そこが exe に埋め込んだ多サイズ `.ico`（ADR 0010）を
使っているためである。**つまり PNG のサイズ選びでは原理的に解決しない。**

### 2. 「テキストエディタで開く」が動かない

`cmd /C start "" <path>` で実装していたが、WinRemap は ADR 0029 で windows サブシステムの
バイナリになり、**標準ハンドルを持たない**。その状態で子プロセスに `cmd` を起動しても、
ハンドルを引き継げず失敗する。v0.1（console サブシステム）では動いていた経路が、
サブシステム変更の副作用で壊れていた。

参照:

- `WM_SETICON`（`ICON_SMALL` / `ICON_BIG` の 2 枠）:
  <https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-seticon>
- `LoadImageW`（サイズ指定で多サイズ .ico から最適な face を取り出す）:
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-loadimagew>
- `EnumThreadWindows`:
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumthreadwindows>
- `ShellExecuteW`:
  <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew>

## 決定

**`src/gui/win32.rs` を新設し、AGENTS.md 不変条件 3 の unsafe 許可リストに追加する**
（オーナー承認済み 2026-07-21）。GUI が必要とするが egui が公開していない Win32 呼び出しを
このファイルに閉じ込める。

内容は次の 2 つだけとする。

1. **`set_window_icons()`** — `EnumThreadWindows` で GUI スレッドの全ウィンドウを列挙し、
   `LoadImageW` で埋め込み `.ico`（ordinal 1）から `SM_CXSMICON` / `SM_CXICON` のサイズを
   読み、`WM_SETICON` で `ICON_SMALL` と `ICON_BIG` の両方を設定する。
   `LR_SHARED` で読むためハンドルの破棄は不要（破棄してはならない）
2. **`open_in_default_editor(path)`** — `ShellExecuteW` で関連付けを起動する。
   中間プロセスも標準ハンドルも不要になり、上記 2 の不具合が構造的に消える

ハンドルを自前で追跡せずスレッドのウィンドウを列挙するのは、ビューポートが何枚あっても
同じコードで済むからである。すでに設定済みのウィンドウへ設定し直しても副作用はない。
呼び出しはウィンドウの開閉が変化した直後の数フレームのみで、毎フレームではない。

egui 側の `ViewportBuilder::with_icon` は使わない（`ICON_SMALL` を上書きするだけで、
2 枠を正しく埋める本方式と競合するため）。

## 理由

- **egui の API では解決できない**。`ICON_BIG` を設定する手段が公開されていない以上、
  生ウィンドウハンドルへ `WM_SETICON` を送る以外に方法がない
- **既に正しい素材がある**。多サイズ `.ico` は exe に埋め込み済みで（ADR 0010）、
  トレイアイコンも exe アイコンもそれを使っている。ウィンドウだけが別素材（PNG）を
  使っていたのが、そもそもの歪みだった
- **`ShellExecuteW` は関連付け起動の正規 API**。`cmd /C start` は「コンソールを持つ前提の
  回避策」であり、常駐 GUI アプリが使うべきものではなかった
- **不変条件 1・2 に触れない**。フックのコールバックにも、そのメッセージループにも関係しない。
  失敗してもアイコンが既定になるかエディタが開かないだけで、リマップは動き続ける
- ADR 0009 の「既存ファイルに畳み込む」判断は当てはまらない。`window.rs` は前面ウィンドウ
  追跡（リマップ本体の一部）であり、GUI の見た目とシェル起動を混ぜると責務が濁る

## 却下した代替案

- **PNG のサイズを調整し続ける**: 48px・32px を実機で試して、いずれも解消しなかった。
  `ICON_BIG` が空である以上、どのサイズでも引き伸ばしは発生する
- **ウィンドウアイコンを設定しない**: 崩れた絵は出なくなるが、Windows 既定のアイコンになり
  WinRemap と判別できなくなる
- **`cmd` に null の標準ハンドルを渡す**（安全な範囲での修正）: 動く可能性はあるが、
  中間プロセスが増える・コンソールが一瞬開きうる・失敗を検知できない、と欠点が残る。
  `ShellExecuteW` なら成否が戻り値で分かり、失敗をユーザーに伝えられる
- **eframe に PR を出して `ICON_BIG` を公開してもらう**: 正攻法だが v0.2 に間に合わない。
  将来 egui 側で対応されたら、本 ADR を新しい ADR で覆してこのファイルを削る余地は残る

## 残るリスク

`EnumThreadWindows` は GUI スレッドが所有する全ウィンドウにアイコンを設定するため、
将来 egui が内部用の隠しウィンドウを作った場合、それにもアイコンが付く。実害はない。
