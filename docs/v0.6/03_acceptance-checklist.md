# v0.6 受け入れチェックリスト（MSIX 固有項目 ＋ 手動最小集合の継承）

> 元資料: [v0.5 受け入れチェックリスト §4](../v0.5/03_acceptance-checklist.md)（手動最小集合 H-1〜H-9 と、v0.1〜v0.3 の仕分け）、
> [v0.6 開発計画 §6](01_development-plan.md)（Phase E）、[ADR 0059](decisions/0059-first-run-creates-the-default-config.md)・[ADR 0060](decisions/0060-msix-package.md)・[ADR 0061](decisions/0061-packaged-config-path.md)、
> [03_release-operations.md §4](../03_release-operations.md)（Store 提出手順）。
> 公式: [Understanding how packaged desktop apps run on Windows](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)（AppData の仮想化）、[Start your app automatically at log-in](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions#start-your-application-at-log-in)（`windows.startupTask`）。

- 作成日: 2026-07-29
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／実施・記録: オーナー

---

## 0. この文書の目的

v0.6.0 は**機能ではなく配布経路のバージョン**である。したがって確認すべきことは 2 種類ある。

1. **これまでどおり動くこと** — 配布形態を変えたせいで既存の挙動が壊れていないこと（**H-1〜H-9** をそのまま継承する）
2. **パッケージ版でしか起きないこと** — 設定パスの仮想化、スタートアップ登録、アンインストール、GitHub 版との共存（**P-1〜P-8** を新設する）

**§1 の手順と、§2・§3 の 2 つの表だけを人が見ればよい。** v0.1〜v0.3 の約 200 項目は v0.5 で仕分け済みで、その結果が H-1〜H-9 である（[v0.5 §3](../v0.5/03_acceptance-checklist.md) が仕分けの根拠）。

### この版で特に注意すること

- **リリースビルドで通す。** Phase A の実測（[開発計画 §2.2](01_development-plan.md)）は debug ビルドで行った。`build.ps1` は既定で `-Configuration release` なので、**オプションを付け足さない**ことがそのまま条件になる
- **自動テストはパッケージ版を見ていない。** `run-vm-ui-test.ps1` が動かすのは素の `winremap.exe` である。パッケージ版の確認は**すべて手動**であり、それが P-1〜P-8 の存在理由である
- **本当の Store 経由インストールは認定通過まで試せない。** P-1〜P-7 は `build.ps1 -Register` で通す。両者の差は署名と配信経路だけで、パッケージ ID・仮想化・マニフェストの解釈は同じである。ただし**「SmartScreen の警告が出ない」ことだけは登録では確認できない**（警告はダウンロードしたファイルに付く印に対して出るものである）。これを P-8 として分け、認定後に行う

---

## 1. 毎リリースの手順

```powershell
cargo test                              # 設定の検証・キー記法・i18n・パス写像
cd tests\ui
.\run-vm-ui-test.ps1                    # ゲスト VM での UI 自動テスト一式
```

1. 上の 2 つが緑であること。**落ちたら手動へ進まない**（v0.5 と同じ規律 — 自動側の判定が壊れていることのほうが重い）
2. **§2 の手動最小集合 H-1〜H-9** を通す（30〜40 分）
3. **§3 の MSIX 固有項目 P-1〜P-7** を通す（20〜30 分）
4. §5 の記録欄に日付と結果を書く
5. **P-8 は認定通過後**に行い、同じ記録欄に追記する

---

## 2. 手動最小集合（v0.5 から継承）

**内容は [v0.5 §4](../v0.5/03_acceptance-checklist.md) と同一である。** 毎回そちらを開かずに済むよう再掲する。「v0.6 での見どころ」の列だけが本バージョン固有で、**その版で何が変わったから何を疑うか**を書いてある。

| # | 何を見るか | 手順の要点 | v0.6 での見どころ |
|---|---|---|---|
| **H-1** | **発端の問題**が配布ビルドで直っていること | PHPStorm の内蔵ターミナル（Claude Code のプロンプト）で `C-h`。1 文字削除になる。**リリース用のバイナリ**と実キーボードで行う | GitHub 版で行う。パッケージ版は P-1 が同じ経路を見る |
| **H-2** | **止まらない・遅れない** | 設定・ログウィンドウを開いたまま、普通の速さで数分間文章を打つ。取りこぼし・二重入力・遅延・スクロールの遅れが無いこと。`C-h` 長押しのリピート、Ctrl 押しっぱなしからの復帰も見る | **v0.5 からの持ち越し**（[ADR 0058](../v0.5/decisions/0058-log-readability.md) 末尾）に注意。開発機では再現しなかったが、ログウィンドウを開いた状態で前面を切り替えたときに `[前面]` 行が出るかを意識して見る |
| **H-3** | **見た目**（1 回で全部見る） | 設定・ログウィンドウとトレイメニューを、**ライトとダークの両方**で開く。罫線・余白・アイコン・注記の印が崩れず判読でき、豆腐（□）が無いこと。日本語表示で未翻訳が無いこと | 新しい UI 文字列が 1 つ増えた（既定設定を作ったときのログ行、[ADR 0059](decisions/0059-first-run-creates-the-default-config.md)）。日英とも出ることは P-1 で見る |
| **H-4** | **放置時の CPU** | 両ウィンドウを閉じて数分放置し、WinRemap の CPU がほぼ 0 であること。数字で見るなら `(Get-Process winremap).TotalProcessorTime` を 5 分空けて 2 回取り、**差が数秒以内** | 変更なし。前回から変更が無い旨を記録に残せば省略可 |
| **H-5** | **IME**（1 回で全部見る） | インジケーターを有効にして、メモ帳・ブラウザ・設定アプリ（UWP）で IME をオン。表示位置・フォーカスを奪わないこと・シェル面で出ないこと。設定ウィンドウの入力欄で日本語変換ができること | 変更なし。ただし**パッケージ版でも 1 度見る**価値がある（前面ウィンドウの取得経路が同じであることの確認） |
| **H-6** | **インストーラー経路** | インストールし、スタートメニューから起動、自動起動を有効にして再サインイン。黒い窓が一瞬も光らないこと | **Inno インストーラー版の話**である。パッケージ版の対応物は P-4。**両方を通す**（v0.6 で経路が 2 つになったため） |
| **H-7** | **マクロ記憶**（1 回で全部見る） | 記憶 → 数文字 → 記憶終了 → 再生。再生しながら別のキーを打ってもリマップが止まらないこと。無効化・リロードで記憶が中断されること。終了後に記憶が消えていること | 変更なし |
| **H-8** | **バナー** | 記憶中のバナーが前面アプリのモニター下部中央に出て、クリックが透過し、Alt+Tab に出ないこと。**マルチモニターと管理者権限ウィンドウ**もここで見る | 変更なし |
| **H-9** | **文書** | README（en/ja）とヘルプサイトが、その版の実挙動と食い違っていないこと | **この版で最も変わった項目。** Phase C で配布経路の記述を全面的に書き換えた。§4 に見る観点をまとめてある |

> H-1・H-2・H-7 は**リマップ本体**の確認で、落ちたらリリースしない。H-3〜H-6・H-8・H-9 は版によって影響を受けない場合があるが、**判断を毎回書き残す**こと（「前回から変更なしのため省略」も記録に残せば可）。

---

## 3. MSIX 固有項目（新設）

### 3.1 事前準備

`%APPDATA%\winremap` の有無で通る経路が変わる（[ADR 0061](decisions/0061-packaged-config-path.md)）ため、**P-1〜P-2 と P-3 では事前状態を作り分ける**。

```powershell
# 退避（P-1・P-2 の前に）
Move-Item $env:APPDATA\winremap $env:APPDATA\winremap.bak

# パッケージ側のデータも消しておく（前回の登録が残っていると初回起動にならない）
Get-AppxPackage -Name SUGANUMADaiki.WinRemap | Remove-AppxPackage
Remove-Item -Recurse -Force `
  "$env:LOCALAPPDATA\Packages\SUGANUMADaiki.WinRemap_pktmgf1zdhxe0" -ErrorAction SilentlyContinue

# 登録（リリースビルド。Developer Mode が要る。証明書も管理者権限も不要）
.\packaging\msix\build.ps1 -Register
```

> **退避したものは必ず戻す。** 全項目を終えたら `Move-Item $env:APPDATA\winremap.bak $env:APPDATA\winremap`。**削除ではなく退避**にしてあるのは、これがオーナーの実運用設定だからである。

### 3.2 項目

| # | 何を見るか | 手順 | 通過条件 | 由来 |
|---|---|---|---|---|
| **P-1** | **Store 版の初回起動** | 設定が無い状態（§3.1 の退避後）でスタートメニューから WinRemap を起動する。トレイからログウィンドウを開き、メモ帳で `C-h` を押す | ①エラーにならず常駐する ②ログに**既定設定を作った旨の行**が出る ③`C-h` が 1 文字削除になる | [ADR 0059](decisions/0059-first-run-creates-the-default-config.md)。**この 1 件が落ちると Store 版は全員が初回起動で失敗する** |
| **P-2** | **設定パス（新規導入）** | トレイ →「設定」。アドレスバーのパスを読む。ドロップダウンから「フォルダーを開く」 | ①アドレスバーが `…\Packages\SUGANUMADaiki.WinRemap_pktmgf1zdhxe0\LocalCache\Roaming\winremap` を指す ②**Explorer がそのフォルダーを実際に開く**（「見つかりません」にならない） | [ADR 0061](decisions/0061-packaged-config-path.md)。表示と実体が一致することの確認 |
| **P-3** | **設定パス（インストーラー版からの移行）** | 一度終了し、`Move-Item $env:APPDATA\winremap.bak $env:APPDATA\winremap` で設定を戻してから起動し直す。設定ウィンドウを開く | ①アドレスバーが `C:\Users\<user>\AppData\Roaming\winremap` を指す ②**自分が書いたキーマップが表示されている**（既定設定に置き換わっていない） ③「フォルダーを開く」でそこが開く | [ADR 0061](decisions/0061-packaged-config-path.md)。**乗り換えても設定が失われない**ことの確認 |
| **P-4** | **スタートアップ** | Windows の**設定 → アプリ → スタートアップ**を開く。WinRemap をオンにしてサインアウト → サインイン | ①一覧に **WinRemap が出る** ②既定は**オフ** ③オンにするとサインイン時に常駐し、**黒い窓が一瞬も光らない** | `AppxManifest.xml` の `windows.startupTask`（`Enabled="false"`）。H-6 のパッケージ版対応物 |
| **P-5** | **アンインストール** | `Get-AppxPackage -Name SUGANUMADaiki.WinRemap \| Remove-AppxPackage` | ①スタートメニューから消える ②`%LOCALAPPDATA%\Packages\SUGANUMADaiki.WinRemap_pktmgf1zdhxe0` が消える ③**`%APPDATA%\winremap` の設定は残る** | ③が要点。GitHub 版と設定を共有している利用者の設定を、パッケージの撤去が巻き込まないこと |
| **P-6** | **単一インスタンス** | パッケージ版を起動した状態で、GitHub 版（`target\release\winremap.exe`）を実行する。逆順でも試す | 2 つ目が**自力で終了**し、トレイアイコンが 1 つのままであること | 二重フックは挙動が不定（README Limitations）。**配布形態が違っても同じ 1 本と見なされる**ことの確認 |
| **P-7** | **管理者権限ウィンドウ** | 管理者として起動したターミナルにフォーカスを当て、`C-h` を押す | リマップが**効かない**（UIPI により通常権限のフックは届かない）。パッケージ版でも GitHub 版と同じ結果になること | 掲載文に「既知の制限」として書いた内容の裏取り（[掲載情報 §1](notes/20260729_store-listing.md)）。**制限が実際にそのとおりであることを確かめる項目**であって、直す項目ではない |
| **P-8** | **Store 経由インストール**（**認定通過後**） | 認定通過後、ストアページ（`https://apps.microsoft.com/detail/9N6TQDXRX5WV`）から**実際にインストールする**。できれば WinRemap を入れたことのない環境で行う | ①**SmartScreen の警告が一切出ない** ②発行元が **SUGANUMA Daiki** と表示される ③P-1 と同じ初回起動が成立する | **本バージョンの目的そのもの**（[開発計画 §0](01_development-plan.md)）。登録（`-Register`）では確認できない唯一の項目であり、ここだけ Phase F にまたがる |

### 3.3 この表に無いこと（意図的に）

| 候補 | 入れなかった理由 |
|---|---|
| `build.ps1 -SelfSign` での署名インストール | 自己署名では **Publisher がマニフェストごと書き換わる**（`build.ps1` がそう作っている）。Store に出すものと別のパッケージ ID になるため、**これを通しても Store 版を通したことにならない**。手元でインストール経路そのものを試したいときの道具として残す |
| パッケージ版の UI 自動テスト | 自動側はゲスト VM に素の exe を送り込む形で、パッケージの登録・撤去を含めると VM の状態管理が別物になる。**費用に見合わない**と判断した（v0.4・v0.5 の「UI テストの CI 組み込み」見送りと同じ性質） |
| 更新（旧版 → 新版）の挙動 | **v0.6.0 が初版**であり、更新する元が無い。v0.7 で初めて確認できる。**そのときの P-9 候補として書き残す** |

---

## 4. H-9（文書）で見る観点

Phase C で配布経路の記述を全面的に書き換えたため、この版の H-9 は分量が多い。**「嘘になっていないか」を見る**のであって、文章の巧拙を見るのではない。

| 見るもの | 確認 |
|---|---|
| `SECURITY.md` | 公式経路が **2 つ**として書かれている。Store 版の検証手段（発行元表示と製品 ID）が書かれている |
| `README.md` / `README.ja.md` | クイックスタートに両経路がある。**Store 版の設定ファイルの場所**が書かれている。スクリーンショット 2 枚が表示される（`site/assets/screenshots/`） |
| ヘルプサイト `install.html`（日英） | 比較表の 4 行が実際の挙動と合っている。SmartScreen の断り書きが **GitHub からのダウンロードに限定**されている |
| ヘルプサイト 全 12 ページ | フッターに**プライバシーポリシーへのリンク**がある |
| ヘルプサイト `index.html` の FAQ（日英） | faq8（保存場所）・faq9（検証）・faq10（経路の違い）が Store 版に対応している |
| **Store リンクの但し書き** | 認定前は「認定通過後に公開されます」が **6 か所**に付いている（[開発計画 §4.3](01_development-plan.md) の一覧）。**認定後はこれが消えていること**が次バージョンの H-9 の確認点になる |
| リンク切れ | 下記が何も出力しないこと |

```powershell
python - <<'PY'
import io,os,re,glob
for f in glob.glob('docs/**/*.md', recursive=True):
    d = os.path.dirname(f)
    for m in re.finditer(r'\]\(([^)\s]+)\)', io.open(f, encoding='utf-8').read()):
        u = m.group(1)
        if u.startswith(('http', 'mailto:', '#')): continue
        if not os.path.exists(os.path.normpath(os.path.join(d, u.split('#')[0]))):
            print(f'{f}: {u}')
PY
```

---

## 5. 記録欄

| 日付 | 対象 | 結果 |
|---|---|---|
| 2026-07-29 | 本チェックリストの作成 | 作成。H-1〜H-9 を継承し、MSIX 固有の P-1〜P-8 を新設 |
| | `cargo test` | |
| | `.\run-vm-ui-test.ps1`（全 9 件） | |
| | §2 手動最小集合 H-1〜H-9 | |
| | §3 MSIX 固有 P-1〜P-7 | |
| | §3 P-8（認定通過後） | |

### 記録の書き方

v0.5 の記録欄（[§6](../v0.5/03_acceptance-checklist.md)）が見本である。**「OK」だけを書かない。** 何をどう確かめたか、省略したなら省略した判断を残す。落ちた項目は、落ちた事実と原因の切り分け結果まで書く — Phase B で 5 回失敗した撮影がそうであったように、**アプリの不具合と手順の不具合は見分けがつかない形で現れる**（[開発計画 §3.5](01_development-plan.md)）。
