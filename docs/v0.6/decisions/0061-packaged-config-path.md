# ADR 0061: パッケージ実行時の設定ファイルパスを起動時に 1 回解決する

- 作成日: 2026-07-29
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- ステータス: 採用
- 関連: [ADR 0060](0060-msix-package.md)（MSIX パッケージ構成。本 ADR はそこで「未解決」として残した課題への回答）、[ADR 0050](../../v0.4/decisions/0050-config-file-switching.md)（設定ファイルの切り替え）、[ADR 0038](../../v0.2/decisions/0038-gui-win32-shell-open.md)（関連付け起動）、[ADR 0051](../../v0.4/decisions/0051-notify-file-watch.md)（設定ファイルの監視）

## 背景

ADR 0060 の Phase 0 実測で、MSIX パッケージ版の設定ファイルが `%LOCALAPPDATA%\Packages\<PFN>\LocalCache\Roaming\winremap\config.toml` にリダイレクトされることを確認した。

このリダイレクトは**アプリ自身からは見えない**。`%APPDATA%\winremap\config.toml` を開けば正しいファイルが返るので、読み書きは何も壊れない。壊れるのは、**そのパス文字列を他プロセスに渡したとき**である。

| 場所 | パッケージ版で何が起きるか |
|---|---|
| 設定ウィンドウ「テキストエディタで開く」 | 起動されるエディタは別プロセス。渡されたパスは存在せず、開けない |
| 同「フォルダーを開く」 | Explorer が存在しないフォルダーを開こうとする |
| アドレスバーの表示と `.toml` 一覧（ADR 0050） | 実在しない場所を指す |
| 起動ログの「… から読み込みました」 | 利用者が Explorer で辿れない場所を案内する |

実測では、パッケージ外から `%APPDATA%\winremap` は**存在しなかった**。つまり利用者から見て「設定ファイルの場所」として表示されているものが、彼らの PC には無い。

## 決定

**起動時に 1 回だけ解決し、以降はその値だけを使う。**

```rust
// src/main.rs
let config_path = std::path::absolute(&cli.config_path).unwrap_or(cli.config_path);
let config_path = package::resolve_config_path(config_path);
```

`src/package.rs` を新設し、次の 2 つを担わせた。

1. `GetCurrentPackageFamilyName` でパッケージ実行かを判定する（unsafe はここ 1 か所）
2. `%APPDATA%` 配下のパスを `%LOCALAPPDATA%\Packages\<PFN>\LocalCache\Roaming\...` へ写像する

解決後のパスは `%APPDATA%` の外にあるため仮想化の対象にならず、**アプリも Explorer も外部エディタも同じ実体を指す**。

### なぜ「境界で変換」ではなく「起動時に 1 回」か

`win32.rs` の `open_in_default_editor` / `open_folder`（パスがプロセスの外に出る唯一の場所）で変換する案もあった。しかしそれでは**表示だけが嘘のまま残る**。アドレスバーが `Roaming\winremap` と表示しながら「フォルダーを開く」が別の場所を開けば、かえって混乱する。

起動時に解決すれば、呼び出し側を 1 つも変えずに、表示・監視・外部起動・切り替えのすべてが同じ 1 つの値に揃う。実装差分は `main.rs` の 1 行である。

### 既存の設定ファイルの扱い

OS の規則をそのまま真似た。

> *"the OS will open the file from the per-user, per-package location first. If that location doesn't exist, then the OS will attempt to open the file from the real `AppData` location."*
> — [Understanding how packaged desktop apps run on Windows](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)

```rust
if redirected.exists() || !path.exists() { redirected } else { path }
```

インストーラー版から Store 版に乗り換えた利用者は `%APPDATA%\winremap\config.toml` を持っている。ここで無条件にリダイレクト先を選ぶと、**その設定が無視されて既定の設定が新規作成される**（ADR 0059）。実在するほうを選ぶことで、乗り換えても設定が引き継がれる。

### `--config` で渡されたパス

`%APPDATA%` 配下でなければ写像せずそのまま返す。`--config D:\work\keys.toml` は利用者自身のファイルであり、パッケージのストアとは無関係である。

## 却下した代替案

- **`RuntimeBehavior="win32App"` にして仮想化そのものを避ける**（ADR 0060 の案 2）: マニフェストの 1 語で済むが、この形が Store 提出で受理されるかを確認できていない。提出して弾かれれば MSIX 構成からやり直しになる。**受理が確実な `packagedClassicApp` のまま、アプリ側で辻褄を合わせるほうが手戻りが小さい**（オーナー決定 2026-07-29）

- **WinRT の `ApplicationData.Current.LocalCacheFolder` で実体を得る**: 最も正確だが、Win32 バイナリから WinRT を呼ぶ足回りが増え、非パッケージ実行では失敗するため結局分岐が要る。得られるものは `%LOCALAPPDATA%\Packages\<PFN>\LocalCache` という、パッケージファミリー名から組み立てられる文字列と同じである

- **設定ファイルの置き場所自体を `%LOCALAPPDATA%` に変える**: 全配布形態で 1 つのパスになり分岐が消えるが、既存利用者全員の設定が移動する。配布形態を増やすための変更が、既存利用者に影響してよい理由はない

- **パッケージ版では設定 GUI の外部起動リンクを隠す**: 症状は消えるが、パッケージ版だけ機能が減る。しかもアドレスバーの表示は依然として嘘のままである

## 不変条件への影響

- **不変条件 2（フックコールバック内の処理制限）**: 変更なし。解決はフック設置より前に 1 回だけ行われる
- **不変条件 3（unsafe の隔離）**: `src/package.rs` を許可リストに追加した（AGENTS.md を更新）。`GetCurrentPackageFamilyName` の 2 段呼び出し（サイズ問い合わせ → 取得）1 か所のみで、`// SAFETY:` 付き。ADR 0031（`notify.rs`）・ADR 0041（`clock.rs`）・ADR 0058（`keyname.rs`）と同じ形 — Win32 に訊かないと答えられない 1 つの問いを 1 モジュールに閉じ込める
- **不変条件 6（キーロガー化の禁止）**: 変更なし

`windows` クレートに feature `Win32_Storage_Packaging_Appx` を追加した（クレート自体の追加ではないため ADR は本件に含める）。

## 確認方法

写像は純粋関数に切り出してあり、`src/package.rs` の単体テスト 3 件で検証している。

| テスト | 保証する内容 |
|---|---|
| `a_config_under_appdata_maps_into_the_package_store` | `%APPDATA%\winremap\config.toml` → `LocalCache\Roaming\winremap\config.toml` |
| `a_path_outside_appdata_is_not_redirected` | `--config D:\...` は写像しない |
| `appdata_itself_maps_to_the_roaming_root` | 境界（`%APPDATA%` そのもの）でも破綻しない |

**表示パスのエンドツーエンド確認は手動で行う。** リダイレクトは読み書きに関して完全に透過であるため、ファイルの配置を外から観察しても修正の前後を区別できない。区別できるのは「アプリが何と表示するか」だけであり、それを見るには設定ウィンドウを開く必要がある。

パッケージ内で `--debug` を走らせて標準出力を捕まえる方法（`Invoke-CommandInDesktopPackage`）も試したが、RPC エラーで起動できなかった。手順:

1. `packaging\msix\build.ps1 -Register`
2. スタートメニューから WinRemap を起動し、トレイから設定ウィンドウを開く
3. アドレスバーが `…\Packages\SUGANUMADaiki.WinRemap_pktmgf1zdhxe0\LocalCache\Roaming\winremap` を指していること
4. 「フォルダーを開く」で Explorer がその場所を実際に開くこと
5. `Get-AppxPackage -Name SUGANUMADaiki.WinRemap | Remove-AppxPackage` で撤去
