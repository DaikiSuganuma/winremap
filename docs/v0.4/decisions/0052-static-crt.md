# ADR 0052: MSVC C ランタイムを静的リンクする（vcruntime140.dll 依存の排除）

- ステータス: 承認（オーナー承認 2026-07-23、winget PR #405731 の検証失敗への対応として）
- 日付: 2026-07-23
- 作成: Claude Code（AI モデル: claude-opus-4-8）
- 参照: [Static and dynamic C runtimes（Rust リファレンス）](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes)、[winget-pkgs PR #405731](https://github.com/microsoft/winget-pkgs/pull/405731)、[Microsoft Visual C++ 再頒布可能パッケージ（Microsoft Learn）](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)

## 文脈

winget への v0.3.0 登録（PR #405731）で自動検証が失敗した。検証 VM がインストール後に winremap.exe を起動したところ、終了コード `-1073741515`（`STATUS_DLL_NOT_FOUND`）で即クラッシュした。

原因はビルド設定にある。Rust の MSVC ターゲットは既定で C ランタイム（CRT）を**動的リンク**するため、winremap.exe は `vcruntime140.dll` をインポートする。この DLL は VC++ 再頒布可能パッケージの一部で、Windows 本体には含まれず、クリーンなマシンには存在しない。開発機や Visual Studio 入りのマシンでは必ず入っているため、これまで顕在化しなかった。

なお UCRT（`api-ms-win-crt-*`）は Windows 10 以降 OS に同梱されており、問題は `vcruntime140.dll` のみ。

v0.3.0 の PR 自体はマニフェストに `Microsoft.VCRedist.2015+.x64` の依存宣言を追加して通す（リリース済みバイナリは変えられない）。本 ADR は v0.4 以降の根治を決める。

## 決定

1. **`.cargo/config.toml` で `x86_64-pc-windows-msvc` ターゲットに `-C target-feature=+crt-static` を設定し、CRT を静的リンクする。** これにより `vcruntime140.dll`（および `api-ms-win-crt-*`）のインポートが消え、exe は Windows 本体の DLL だけで起動する。
2. **v0.4.0 以降の winget マニフェストでは `Microsoft.VCRedist.2015+.x64` の依存宣言を外す。** 依存宣言は v0.3.0 のマニフェストにだけ残る。
3. リリース手順への影響はない（`cargo build --release` のまま。設定はリポジトリにコミットされ、CI・手元ビルドの両方に自動で効く）。

## 理由

- **「インストールすれば動く」を配布物単体で保証できる。** 依存宣言による解決は winget 経由のインストールにしか効かず、GitHub Releases からセットアップ exe を直接落とした利用者は同じクラッシュを踏む。静的リンクなら配布経路によらず直る。
- **安定性優先の原則に合う。** 実行時の外部条件（VCRedist の有無・バージョン）への依存が 1 つ消える。トレードオフはバイナリサイズの増加（数百 KB 程度)だけで、常駐アプリとして問題にならない。
- **設定 1 箇所・コード変更ゼロで済む。** `.cargo/config.toml` はリポジトリにコミットされるため、ビルドする環境すべてで同じ結果になる。

## 却下した代替案

- **winget マニフェストの依存宣言だけで対応し続ける**: v0.3.0 の暫定対応としては正しいが、GitHub Releases 直接ダウンロードの利用者を救えない。根治にならない。
- **インストーラー（Inno Setup）に VCRedist を同梱・チェーンインストール**: インストーラーが肥大化し、VCRedist のバージョン追従という保守作業が生まれる。静的リンクなら不要。
- **`vcruntime140.dll` を exe と同じフォルダーに同梱**: VC++ ランタイムの再頒布ルールへの配慮が必要になり、更新も手動になる。静的リンクの方が単純。
- **`RUSTFLAGS` 環境変数や CI 側での指定**: ビルドする場所ごとに設定が要り、手元ビルドとリリースビルドで挙動が食い違う余地を残す。リポジトリ内の `.cargo/config.toml` が唯一の置き場所として正しい。
