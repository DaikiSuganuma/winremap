# ADR 0048: scoop 配布を保留する（Extras の知名度基準を満たすまで）

- ステータス: 承認（オーナー指示）
- 日付: 2026-07-22
- 作成: Claude Code（AI モデル: claude-opus-4-8）

## 文脈

[ADR 0045](0045-package-manager-channels.md) で「scoop は公式 [Extras](https://github.com/ScoopInstaller/Extras) バケットへ提出する」と決め、v0.3.0 リリース後にマニフェスト（`packaging/scoop/winremap.json`）を提出した（[ScoopInstaller/Extras#18357](https://github.com/ScoopInstaller/Extras/pull/18357)）。

これは **Extras メンテナによりクローズ**された（2026-07-22、`not-meet-criteria`）。理由は Extras の受け入れ基準:

> Reasonably well-known and widely used (e.g. if it's a GitHub project, it should have at least 100 stars and/or 50 forks)

WinRemap は公開されたばかりで、この星 100／fork 50 の目安に達していない。メンテナからは「community バケットへ寄稿するか、自前バケットを作るか、基準を満たしてから（package request を出して）再申請を」と案内された。

ADR 0045 は winget と scoop の両方を扱っていたが、**winget 側（[microsoft/winget-pkgs#405731](https://github.com/microsoft/winget-pkgs/pull/405731)）はこの種の知名度基準を持たず影響を受けない**。変わったのは scoop の提出先だけなので、その部分だけを本 ADR で上書きする。

## 決定

1. **scoop 対応を保留する。** いまは自前バケットを作らず、Extras への再申請もしない

2. **README・ヘルプサイトから scoop の記述を外す。** 案内するのは winget（公開後）と GitHub Releases（常時）のみとする。実際に入らない手順を載せない

3. **`packaging/scoop/winremap.json` は残す。** 再申請時にそのまま使える。`packaging/README.md` に「保留中」と明記する

4. **再開条件**: WinRemap が Extras の基準（星 100／fork 50 目安）に達したら、まず Extras に [package request](https://github.com/ScoopInstaller/Extras/issues/new?template=package-request.yml) を出し、承認の見込みが立ってからマニフェストを再提出する

5. **ADR 0045 の scoop 部分（決定 2、および決定 3 のうち scoop）は本 ADR で置き換える。** winget の決定（ID `DaikiSuganuma.WinRemap`、`Moniker: winremap`、`InstallerType: inno`、リリース後提出）は引き続き有効

## 理由

- **自前バケットは維持コストと導入の一手間を生む。** ADR 0045 で自前バケットを却下した理由（利用者に `scoop bucket add` を踏ませる、維持リポジトリが増える、「公式サイト以外から入れている」感触）は、Extras がだめでも消えない。knownness 基準を満たせば Extras に入れられるのだから、それまで待つほうが筋が良い（オーナー決定）
- **winget と GitHub Releases で導入経路は足りている。** winget が Windows 標準のパッケージマネージャーであり、scoop が無くても利用者は困らない
- **偽の手順を残さないのが最優先。** 「入らない `scoop install`」を README やサイトに載せ続けるのは、利用者を迷わせるだけである
- **マニフェストは完成しており、再申請は安い。** 破棄せず残せば、基準到達後に URL とハッシュを更新するだけで再提出できる

## 却下した代替案

- **自前バケット（`DaikiSuganuma/scoop-winremap` など）をいま作る**: リリース直後から `scoop install` を通せる。しかし上記の維持コスト・導入の一手間があり、基準を満たせば Extras に入れられる見込みがあるため、オーナー判断で見送る。方針を変えるなら本 ADR を上書きする ADR を書く

- **本体リポジトリを bucket として使う**（`bucket/winremap.json` を winremap リポジトリ直下に置く）: 追加リポジトリは要らないが、`scoop bucket add` でアプリ本体のリポジトリ全体がクローンされ、バケットとしては重い。やはり見送る

- **Extras に食い下がる／再オープンを求める**: 明確な基準に基づくクローズであり、基準未達のまま押し返す理由がない

## 参照（公式）

- Scoop Extras 受け入れ基準（package request テンプレート）: https://github.com/ScoopInstaller/Extras/issues/new?template=package-request.yml
- Scoop バケットの作り方: https://github.com/ScoopInstaller/Scoop/wiki/Buckets#creating-your-own-bucket
- クローズされた提出 PR: https://github.com/ScoopInstaller/Extras/pull/18357
