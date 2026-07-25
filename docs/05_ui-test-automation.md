# UI テスト自動化（VM ＋ AI エージェント）

> 元資料: windows-utility リポジトリの `.ai-rules/rules/workflows/windows-ui-testing.md`（Windows デスクトップアプリの UI テスト自動化・共通手順）と
> `docs/projects/20260723_Windowsアプリテスト環境構築/11_UIテスト自動化ガイド(WinRemapエージェント向け).md`（WinRemap 固有の差分）。
> 環境の構築・保守はそちらが正であり、本書は**このリポジトリ側の使い方**を記述する。

- 作成日: 2026-07-25
- 作成: Claude Code（AI モデル: claude-opus-5）／レビュー・承認: オーナー
- 関連: [ADR 0053](v0.4/decisions/0053-test-inject-mode.md)（注入イベント受理モード）、[v0.4 開発計画 §6](v0.4/01_development-plan.md)（Phase E）

---

## 1. これは何か

VMware のゲスト Windows（自動ログオン済みのデスクトップ）で Claude Code ＋ Terminator MCP（Model Context Protocol: AI が外部ツールを呼ぶための規格）に画面を操作させ、WinRemap の GUI とリマップ動作を自動で検証する。UI 自動化は画面のある対話セッション（session 1）でしか動かず、常駐アプリとグローバルフックを毎回きれいに消す必要があるため、VM とスナップショットを使う。

実行の入口はこのリポジトリの `tests/ui/run-vm-ui-test.ps1`（ホスト側で実行）。ゲストへコマンドを流す部分は windows-utility の `test-vm/scripts/run-in-vm-vmware.ps1` に委譲する。

```
run-vm-ui-test.ps1（本リポジトリ）
  ├─ vmrun revertToSnapshot ready → start   … 前回の残骸を消す
  ├─ cargo build --release [--features test-inject]
  ├─ vmrun copyFileFromHostToGuest          … winremap.exe と minimal.toml を C:\Test へ
  ├─ run-in-vm-vmware.ps1 -Command "claude -p <シナリオ>"（windows-utility）
  │    └─ ゲスト session 1 で Terminator MCP が UI を操作
  └─ 最終行の PASS / FAIL を集計 → revert
```

## 2. 前提

| 項目 | 値 |
|---|---|
| ホスト | VMware Workstation（Hyper-V の合成ディスプレイは OpenGL 非対応で設定画面が出ない） |
| ゲスト VM | `C:\VMware\win11-test\win11-test.vmx`（接続情報は windows-utility の `.secrets/test-vm.json`） |
| ゴールデンスナップショット | `ready`（ツール導入・認証・Terminator 込み） |
| ゲスト内 | Claude Code（`CLAUDE_CODE_OAUTH_TOKEN` を User 環境変数に）、Terminator MCP `0.24.28` |

**認証情報・VM の接続情報は本リポジトリに置かない。** スクリプトは windows-utility の `.secrets/test-vm.json` を読むだけで、パスワードを出力しない。

## 3. 実行方法

```powershell
cd D:\Projects\GitHub\winremap\tests\ui
.\run-vm-ui-test.ps1                              # 全シナリオ
.\run-vm-ui-test.ps1 -Scenario 05-remap-notepad   # 1 本だけ
```

| オプション | 既定 | 用途 |
|---|---|---|
| `-Scenario` | `all` | `scenarios\` のファイル名（拡張子なし） |
| `-EntryScript` | windows-utility の既定パス | 別マシンでリポジトリの場所が違う場合に指定 |
| `-Snapshot` | `ready` | 戻す先のスナップショット |
| `-TimeoutMin` | `15` | 1 シナリオあたりの上限 |
| `-NoRevert` | — | revert せず今のゲストで実行（**プロンプト調整中のみ**。再現性は失われる） |
| `-SkipBuild` | — | ビルド済みバイナリを再利用 |

終了コードは全シナリオ PASS で 0、1 つでも FAIL または判定不能なら 1。

## 4. シナリオ

`tests/ui/scenarios/*.txt` が 1 ファイル 1 シナリオで、中身はそのまま `claude -p` に渡す英語プロンプトである。**最終行に `PASS` か `FAIL` だけを出す**よう指示してあり、ランナーはそれを拾う。

| ファイル | 内容 | 対応する受け入れ項目 |
|---|---|---|
| `01-settings-window` | 設定画面が開いて描画される（OpenGL） | 設定ウィンドウの起動 |
| `02-config-display` | アドレスバーのパスとキーマップ表示が `minimal.toml` と一致する | 設定内容の表示 |
| `03-tray-actions` | 有効/無効トグル・Reload・Quit | トレイメニュー |
| `04-log-window` | ログウィンドウに起動ログ（読み込んだキーマップ数とパス） | ログ表示 |
| `05-remap-notepad` | Notepad で `C-h` → Backspace（文字が消え、置換ダイアログが出ない） | リマップ動作 |

起動は必ず `--config C:\Test\minimal.toml --lang en` を付ける。**`--lang en` はゲストのロケール（日本語）に関係なく UI 文字列を英語に固定する**ためで、これが無いとプロンプト中の要素名が環境依存になる。

### 決定論フェーズと探索フェーズ

共通手順の原則どおり、**確定した手順に LLM を挟まない**。現行のプロンプトは操作手順を明示した第 1 版であり、実行して要素の role/name が判明したら**その名前をプロンプトに焼き込んで**探索の余地を減らす。プロンプトを変えたら、変更前後で同じ判定になることを 2 回以上の実行で確かめる。

## 5. リマップ検証だけが特別な理由

WinRemap は自己送出ループを防ぐため、注入イベント（`LLKHF_INJECTED`）を素通しする（AGENTS.md 不変条件 1）。Terminator の `SendInput` にもこのフラグが立つため、**通常のビルドではテストのキー送出はリマップされない**。

そこで、既定 OFF の Cargo feature `test-inject` を有効にしたビルドでのみ `--accept-injected` を受け付け、**自分が注入したイベント（`dwExtraInfo` のマーカー付き）以外**の注入を変換対象にする（[ADR 0053](v0.4/decisions/0053-test-inject-mode.md)）。自分の注入は従来どおり無条件で素通しするので、自己送出ループは閉じたままである。

- ランナーは、プロンプトに `--accept-injected` が含まれるシナリオを自動的に `--features test-inject` でビルドする。それ以外は出荷物と同じ形のビルドで実行する
- **配布物にはこのコードが入らない。** `cargo build --release`（feature 無し）ではフラグ自体が存在せず、`--accept-injected` は不明な引数として拒否される（`src/main.rs` の単体テストで担保）
- このモードで起動した実行ファイルは、起動ログとトレイメニューの先頭行に `TEST BUILD` を出す

## 6. ハマりどころ

| 事象 | 対処 |
|---|---|
| 設定画面が黒い・出ない | Hyper-V では OpenGL が無い。VMware で実行する（`08_egui設定画面のOpenGL問題.md`） |
| ゲストで `claude` が見つからない | PATH 更新が既存セッションに反映されていない。ゴールデンスナップショットはツール導入後に再起動した状態にする |
| 日本語が文字化けする | ゲストの PowerShell 5.1 は BOM 無し UTF-8 を cp932 として読む。**プロンプトは英語で書く**（入口スクリプトは BOM 付きで転送する） |
| UI 自動化が無反応 | session 1（対話デスクトップ）でしか動かない。入口スクリプト経由で実行する |
| ビルドが `アクセスが拒否されました` で失敗 | ホストで `target\release\winremap.exe` が起動中。終了させる（`-SkipBuild` で回避も可） |
| 判定が毎回ぶれる | プロンプトに探索の余地が残っている。§4 のとおり要素名を焼き込む |

## 7. やらないこと

- **CI（GitHub Actions）への組み込み**: VMware とゲストの認証情報がホスト固有であり、手元実行に留める（v0.4 時点）
- **キー入力内容のログ収集**: テストのためであってもキーロガー化はしない（不変条件 6）
- **`--accept-injected` の配布物への露出**: 利用者向けドキュメント（README・ヘルプサイト）には載せない
