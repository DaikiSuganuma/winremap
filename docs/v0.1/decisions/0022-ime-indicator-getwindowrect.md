# ADR 0022: インジケーターの位置計算は GetWindowRect に統一（DWM 矩形の不採用）

- ステータス: 承認（オーナー検証を受けた実装修正）
- 日付: 2026-07-19
- 作成: Claude Code（AI モデル: claude-fable-5）

## 文脈

設計書 §3.3 は当初、見た目どおりの矩形が取れる `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` を優先し、`GetWindowRect` をフォールバックとしていた。しかし `DWMWA_EXTENDED_FRAME_BOUNDS` は **DPI 仮想化を無視した物理ピクセル座標**を返す（Microsoft Learn の DWMWINDOWATTRIBUTE ドキュメントに明記）。

winremap は DPI 非対応（マニフェストなし、計画書 Phase I3 タスク 6）で、プロセス内の座標系（`GetWindowRect` / `SetWindowPos`）は仮想化された論理座標である。表示スケール 100% 超のモニターでは物理座標が論理座標より大きくなるため、DWM 矩形から計算した中央にパネルを置くと**画面外や大きくずれた位置に表示される**。Phase I3 の「パネルが表示されない」報告の一因になり得る（主因は ADR 0021 のトリガー不発）。

## 決定

位置計算は `GetWindowRect` のみとし、DWM 矩形は使わない。`Win32_Graphics_Dwm` feature も削除する。

## 理由

- `GetWindowRect` は自プロセスの座標系と常に一致し、スケーリング環境でも正しい位置に表示される
- DWM 矩形との差は不可視のリサイズボーダー（片側数 px）だけで、パネル中央位置への影響は実用上見えない
- 「DPI は OS の仮想化に任せる」という Phase I3 タスク 6 の判断と整合する

## 却下した代替案

- **Per-Monitor V2 DPI 対応にして DWM 矩形を使う**: 座標系は一致するが、DPI 対応はトレイ・今後の UI を含むプロセス全体の描画に影響し、数 px の精度のために背負う変更として大きすぎる。高 DPI でのシャープな描画が必要になったときに改めて判断する
- **PhysicalToLogicalPointForPerMonitorDPI で物理→論理変換**: 追加 API と失敗系が増えるわりに得られる精度が数 px で、複雑さに見合わない
