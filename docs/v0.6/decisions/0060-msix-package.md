# ADR 0060: Microsoft Store 向け MSIX パッケージの構成

- 作成日: 2026-07-29
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／レビュー・承認: オーナー
- ステータス: 採用
- 関連: [ADR 0027](../../v0.1/decisions/0027-windows-installer-inno-setup.md)（MSIX を却下した判断を**更新する**）、[ADR 0045](../../v0.3/decisions/0045-package-manager-channels.md)（同）、[ADR 0059](0059-first-run-creates-the-default-config.md)（初回起動時の設定生成）、[ADR 0010](../../v0.1/decisions/0010-icon-embedding.md)（アイコンの出所）

## 背景

配布物に SmartScreen の「Windows によって PC が保護されました」が出る問題への対応として、オーナーが Microsoft Store 配信を決定した（2026-07-29）。

Microsoft の公式ドキュメントは、非 Store 配信では**署名しても警告は消えない**と明示している。評判はダウンロード数で蓄積するもので、かつて即座に信頼を得られた EV 証明書も *"EV certificates no longer bypass SmartScreen"* と否定された（[SmartScreen reputation for Windows app developers](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)）。**警告が出ないことを保証する経路は Store のみ**である。

MSIX は ADR 0027・0045 で 2 度却下している。当時の理由と、現在の評価は次のとおり。

| 当時の却下理由 | 現在 |
|---|---|
| 署名証明書が必須で費用がかかる | **逆だった**。Store 配信では Microsoft が再署名するため、証明書は不要かつ購入不可 |
| ストア審査と MSIX 化の手間 | 個人開発者の登録料は無償化済み。MSIX 化は本 ADR の構成で完了 |
| 低レベルフック常駐アプリと AppContainer の相性が未検証 | **本 ADR で実測した**（下記）。AppContainer に入れる必要がそもそも無い |

却下の前提が変わったため、両 ADR の判断を本 ADR で更新する（当該 ADR 自体は書き換えない）。

## 決定

### 1. AppContainer ではなく full trust の Desktop Bridge パッケージにする

```xml
<Application uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="packagedClassicApp">
<rescap:Capability Name="runFullTrust" />
```

`WH_KEYBOARD_LL` と `SendInput` は Win32 API であり、WinRT の入力注入 API（`Windows.UI.Input.Preview.Injection`、`inputInjectionBrokered` が必要）とは別物である。full trust の packaged app は通常の medium-IL プロセスとして動くため、フックまわりのコードに変更は要らない。

`RuntimeBehavior` に `win32App`（仮想化されない）ではなく `packagedClassicApp` を選んだのは、後者が Store で実績のある形だからである。仮想化の副作用（決定 5）は承知のうえで、まず確実に通る構成を採る。

### 2. パッケージアセットは SVG から生成し、コミットする

`examples/msix_assets.rs` が `assets/svg/kbd-enabled.svg` から 23 枚の PNG を書き出す。トレイアイコン（ADR 0010）と同じ 1 枚の SVG が出所なので、ストアの見た目と動いているアプリの見た目が食い違わない。

既存の `assets/png/*-256.png` を縮小しなかったのは、150x150 タイルの scale-400 が 600px であり、256px の原本から作れないためである。

生成物をコミットするのは、パッケージ作成に Rust ツールチェーンを要求しないためである。アイコンを描き直したときだけ再生成する。

### 3. 自動起動は `windows.startupTask`（既定 OFF）

Inno インストーラーは `{userstartup}` にショートカットを置いている（`installer/winremap.iss`）。パッケージアプリはスタートアップフォルダに書き込めないため、マニフェストの拡張で宣言する。利用者は Windows の「設定 → アプリ → スタートアップ」で切り替えられる（ショートカット方式には無かった利点）。

`Enabled="false"` にしたのは、頼まれてもいないのにサインイン時から常駐するキーリマッパーが第一印象として良くないためである。

### 4. パッケージは自分で署名しない

Store 提出物は未署名でよく、Microsoft が認定後に再署名する。**署名すると差し戻される**。`build.ps1 -Pack` は署名せずに終わる。

### 5. ローカル検証は Developer Mode ＋ `Add-AppxPackage -Register`

自己署名証明書を作って署名し、`TrustedPeople` に入れて…という経路は管理者権限を要求する。レイアウトフォルダをそのまま登録すれば、証明書も管理者権限も要らない。`build.ps1 -Register` がこれを行う。署名インストールの経路自体を試したい場合のために `-SelfSign` も残した。

### 6. バージョンは `Cargo.toml` から取り、第 4 フィールドは 0

Store がリビジョン部を自分で使うため、`0.6.0.0` の形にする。`build.ps1` が `Cargo.toml` を読んで埋めるので、番号が二重管理にならない。

## Phase 0 実測（2026-07-29、Windows 11 Pro 26200、debug ビルド）

`build.ps1 -Register` で登録し、実機で測った。

| 検証項目 | 結果 |
|---|---|
| パッケージ登録 | ✅ PFN が `SUGANUMADaiki.WinRemap_pktmgf1zdhxe0` となり、Partner Center 発行値と一致。Identity の 2 値が正しいことの証明になる |
| 起動とキーボードフックの設置 | ✅ 常駐した。`hook::install()` が失敗すれば `run()` はそこで終了するので、**プロセスが生きていること自体がフック設置成功の証拠**である |
| 初回起動時の設定生成（ADR 0059） | ✅ 生成された。この修正が無ければ、ここで起動に失敗していた |
| 設定ファイルの実体 | ⚠️ `%LOCALAPPDATA%\Packages\<PFN>\LocalCache\Roaming\winremap\config.toml` にリダイレクトされた |
| パッケージ外から見た `%APPDATA%\winremap` | ⚠️ **存在しない** |
| 単一インスタンス防止 | ✅ 2 つ目のプロセスは自力で終了し、1 つだけが残った |
| キー変換の実動作 | **未検証**。注入イベントは不変条件 1 により素通しするため、合成入力では試せない。人が実際にキーを押す必要がある |

### 判明した課題（未解決）

設定ファイルの表示パスが実体と食い違う。アプリは `%APPDATA%` を解決して `C:\Users\<user>\AppData\Roaming\winremap\config.toml` と表示するが、そのフォルダは**パッケージ外のプロセスからは存在しない**。したがってパッケージ版では次が壊れる。

- 設定ウィンドウの「エディタで開く」— 起動される外部エディタはパッケージ外のプロセスなので、実体に届かない
- 「フォルダを開く」・アドレスバーの表示と `.toml` の一覧（ADR 0050）
- 起動ログの `... から読み込みました` の行

対処は 2 案あり、次の作業で決める。

1. `GetCurrentPackageFullName()` でパッケージ実行を判定し、表示・外部起動に使うパスだけ実体（`LocalCache\Roaming\...`）に解決する
2. `RuntimeBehavior="win32App"` に変えて仮想化そのものを避ける。ただし Store 提出でこの形が受理されるかは未確認

## 却下した代替案

- **EXE/MSI 形式で Store に出す**: Store は再署名せず、全 PE ファイルが Microsoft Trusted Root にチェーンする証明書で署名済みであることを提出要件とする（[App package requirements for MSI/EXE app](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements)）。証明書を持たない現状では提出できず、仮に持っていても SmartScreen 警告は残るので目的を果たさない

- **MSIX Packaging Tool で既存の Inno インストーラーをキャプチャする**: インストール操作を記録して差分をパッケージ化する方式。exe 1 本とアイコンだけの構成でこれを使うと、記録時の環境が混入する余地が生まれ、CI にも載せにくい。マニフェストを手で書くほうが見通しがよい

- **アセットを `build.rs` で生成する**: resvg は既にビルド依存にあるが、`build.rs` からリポジトリ配下に書き出すのは行儀が悪く、生成物をコミットする方針とも噛み合わない。`examples/` は既に開発用ツールの置き場になっている（`ime_probe.rs` 等）ので、そこに置いた

- **`Enabled="true"` で自動起動を既定 ON にする**: インストーラーはチェックボックスで**利用者に訊いている**。パッケージ版で黙って ON にすると、同じ製品が入手経路によって違う振る舞いをすることになる

## 不変条件への影響

- **不変条件 1（自己送出ループの防止）**: 変更なし。パッケージ化は配布形態の話で、フックのコードに触れていない
- **不変条件 2（フックコールバック内の処理制限）**: 変更なし
- **不変条件 3（unsafe の隔離）**: 変更なし。追加した Rust コードは `examples/msix_assets.rs` のみで、unsafe を含まない
- **不変条件 5（既知の制限の明文化）**: 管理者権限ウィンドウに効かない件（UIPI）はパッケージ版でも同じ。Store の説明文にも記載する
- **禁止事項（ネットワーク通信）**: 変更なし。Store の自動更新は OS が行い、アプリは何も通信しない

## 依存の追加

`resvg` を `[dev-dependencies]` に追加した。同じクレートが既に `[build-dependencies]` にあり（ADR 0040 で審査済み）、examples からはビルド依存が見えないための重複エントリである。新規クレートの導入ではないため、ライセンス・保守状況の再審査は行っていない。
