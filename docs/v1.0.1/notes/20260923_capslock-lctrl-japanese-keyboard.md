# CapsLock → LCtrl の単キールールで Ctrl が押しっぱなしになる（日本語キーボード種別の調査）

- 作成日: 2026-09-23
- 作成: Claude Code（AI モデル: claude-fable-5-1）／報告・判断: オーナー
- 対象: `"CapsLock" = "LCtrl"`（単キールール、[v0.1 設定仕様 §3.2](../../v0.1/02_config-spec.md)）、[ADR 0005](../../v0.1/decisions/0005-modifier-lift-restore.md)・[ADR 0006](../../v0.1/decisions/0006-injection-markers.md)
- 再現スクリプト: [20260923_capslock-probe.ps1](20260923_capslock-probe.ps1)（低レベルフック）、[20260923_capslock-rawinput-probe.ps1](20260923_capslock-rawinput-probe.ps1)（Raw Input 併用）
- 参考: [Names for Japanese special keys（AutoHotkey Community）](https://www.autohotkey.com/boards/viewtopic.php?t=40513)、[Ctrl + Shift + Capslock でかな入力に切り替わるのを防止する方法（Microsoft Q&A）](https://learn.microsoft.com/ja-jp/answers/questions/4294453/ctrl-shift-capslock)、[JP Japanese Keyboard Layout for 101 – Virtual Keys（kbdlayout.info）](http://kbdlayout.info/kbd101/virtualkeys)

## 1. 結論

**WinRemap の不具合ではなく、Windows の日本語キーボードドライバーの挙動が原因**である。日本語キーボード種別（`OverrideKeyboardType = 7`。US 101 配列を日本語 IME で使う `kbd101.dll` も含む）では、**Ctrl や Shift を押したまま CapsLock を離すと、その解放イベント（キーアップ）が低レベルフックにも Raw Input にも届かない**。WinRemap は CapsLock↓で LCtrl↓を注入するので、利用者が CapsLock を離す瞬間は必ず「Ctrl 押下中」になり、解放が捨てられて LCtrl↑を注入する契機が永遠に来ない。CapsLock を 1 回タップしただけで Ctrl が押しっぱなしになる。

レジストリの Scancode Map で CapsLock→Ctrl が問題なく動いていたのは、あれがドライバー層（kbdclass）でスキャンコード 0x3A を 0x1D に書き換えるため、この読み替え処理に CapsLock が到達しないからである。

入力言語を **英語 (米国)**（`kbdus.dll`）にしている間は解放が正常に届き、単キールールはそのまま動く。日本語 IME を有効にしている間は日本語のレイアウトに戻るので、問題も戻る。

## 2. 環境

| 項目 | 値 |
|---|---|
| OS | Windows 11 Pro 10.0.26200 |
| WinRemap | v1.0.1（Store 版、`personal-ja.toml`） |
| キーボード | US 101 配列。`HKLM\SYSTEM\CurrentControlSet\Services\i8042prt\Parameters`: `LayerDriver JPN = kbd101.dll`、`OverrideKeyboardType = 7`、`OverrideKeyboardIdentifier = PCAT_101KEY`（Windows の「ハードウェア キーボード レイアウト: 英語キーボード (101/102 キー)」がこの値を入れる） |
| Scancode Map | `HKLM\...\Control\Keyboard Layout\Scancode Map` は 12 バイトすべて 0（CapsLock→Ctrl の登録を外し、当日 8:11 に再起動済み） |
| 入力言語 | 日本語（0411、Microsoft IME）と 英語 (米国)（0409、US）の 2 つが登録済み |

## 3. 症状（オーナー報告）

レジストリの CapsLock→Ctrl を外し、`personal-ja.toml` に `"CapsLock" = "LCtrl"` を書いたところ、Ctrl が押しっぱなしになったような動作になった。

## 4. 再現

### 4.1 WinRemap を通した再現

1. Store 版を止め、`cargo build --release --features test-inject` のバイナリを `--config caps.toml --accept-injected --debug` で起動（`caps.toml` は `application = ["*"]` に `"CapsLock" = "LCtrl"` と `"C-a" = "Home"` の 2 規則）
2. 使い捨ての `cmd.exe` を前面にして、`SendInput` で CapsLock / a を注入（`test-inject` ビルドは他ソフトの注入を物理キーと同じに扱う。ADR 0053）
3. 別プロセスの低レベルフックで配送されたイベント列を採り、`GetAsyncKeyState(VK_LCONTROL)` で最終状態を見る

CapsLock 単独タップ（↓、30 ms 後に↑）の結果。フックに届いたのは 2 件だけで、**CapsLock↑が来ない**。

```
+   0ms INJ   DOWN vk=0x14 sc=0x3A   ← 物理押下相当。WinRemap が抑止し LCtrl↓ を注入
+   0ms REMAP DOWN vk=0xA2 sc=0x1D   ← 注入された LCtrl↓
after: LCtrl async=DOWN               ← 押しっぱなし
```

WinRemap 側の `--debug` ログも `[入力] CapsLock ↓` → `[判定] CapsLock → LCtrl に差し替え` → `[注入] LCtrl ↓` で止まり、`CapsLock ↑` の行が無い。

| 注入した順序 | 結果 |
|---|---|
| CapsLock↓ → CapsLock↑ | **Ctrl が残る** |
| CapsLock↓ → a↓ → a↑ → CapsLock↑ | **Ctrl が残る**（C-a→Home は正しく出る） |
| CapsLock↓ → a↓ → CapsLock↑ → a↑ | 正常（a↓の修飾補正で Ctrl が離れている瞬間に CapsLock↑が来るので届く） |

3 行目が正常なことから、解放さえ届けば ADR 0005/0006 の一時解除・復元と修飾キー追跡は正しく機能している。設定ファイルや keymap の解決にも問題は無い。

### 4.2 Windows 単体の切り分け（WinRemap 不使用）

Store 版（注入イベントは素通し）が動いたまま、同じフックプローブで Windows の挙動だけを見た。

| 注入した列 | フックに届いたもの |
|---|---|
| CapsLock↓↑（単独） | 0x14↓ 0x14↑ 正常 |
| LCtrl↓ → CapsLock↓↑ → LCtrl↑ | CapsLock↓が **0xF2（VK_OEM_COPY＝カタカナ）** に化け、CapsLock↑は**消える** |
| CapsLock↓ → LCtrl↓ → CapsLock↑ → LCtrl↑ | CapsLock↓は 0x14。CapsLock↑は**消える** |
| LShift↓ → CapsLock↓↑ → LShift↑ | 0xF2↑ と **0xF0（VK_OEM_ATTN＝英数）**↓ に化け、CapsLock↑は**消える** |
| LCtrl↓ → F7↓↑ → LCtrl↑ | 正常 |
| LCtrl↓ → ScrollLock↓↑ → LCtrl↑ | 正常（CapsLock 固有の現象） |

これは JP 101 キーボードの IME キー割り当て（Ctrl+CapsLock＝カタカナ、Shift+CapsLock＝英数、Alt+CapsLock＝ひらがな、Alt+`＝半角/全角）を `kbd101.dll` の NLS テーブルと win32k が実現している処理で、修飾キー押下中の CapsLock を別の仮想キーに読み替え、解放は送らない。

副作用として、解放が捨てられた後は Windows が CapsLock を「物理的に押されたまま」とみなすため、次の CapsLock↓がリピート扱いになりトグルが反転しない（プローブの後片付けで 2 回タップが要った）。

### 4.3 Raw Input でも見えない

`RegisterRawInputDevices`（`RIDEV_INPUTSINK`）で `WM_INPUT` を併せて採った。LCtrl 押下中の CapsLock↑は **Raw Input にも来ない**（`make=0x3A` の BREAK が無い）。win32k がフック・Raw Input の両方より手前で捨てているので、ユーザーモードから解放を観測する手段は無い。

### 4.4 入力言語を英語 (米国) にすると正常

前面ウィンドウに `WM_INPUTLANGCHANGEREQUEST` で 0409（`kbdus.dll`）を適用して同じ列を流した。

| 入力言語 | LCtrl 押下中に CapsLock↓↑ | LShift 押下中に CapsLock↓↑ |
|---|---|---|
| 日本語（kbd101.dll） | ↓が 0xF2 に化け、↑は消える | ↓が 0xF0 に化け、↑は消える |
| 英語 (米国)（kbdus.dll） | 0x14↓ 0x14↑ 正常 | 0x14↓ 0x14↑ 正常 |

レイアウトは前面スレッドの入力言語で決まるので、`Win + Space` で英語 (米国) にしている間だけ単キールールが動く。

## 5. 否定した仮説

- **注入した LCtrl↑の追跡が非同期で、既にキューにある a↑と競合する**（ADR 0006 が「注入前に状態を書き換えない」を選んだことによる競合）: 4.1 の 3 行目で、注入した LCtrl↑はキュー済みの a↑より先にフックへ戻り、`SIDES` が先に更新されていた（a↑の復元で LCtrl↓が出ていない）。競合は起きていない
- **JIS 配列の 英数/CapsLock キーが別 VK になる**: 開発機は US 101 配列で、CapsLock 単独は 0x14 で届く。原因は配列ではなくキーボード種別

## 6. 選択肢

1. **CapsLock だけレジストリの Scancode Map に戻す**（確実。他のリマップは WinRemap のまま併用できる）
2. **入力言語を英語 (米国) にしている間だけ使う**（4.4。日本語 IME を使っている間は使えない）
3. **WinRemap に「仮想修飾キー」方式を実装する**: CapsLock 押下中は LCtrl を注入せず内部の修飾キー状態だけ Ctrl 扱いにし、他キーが押されたときだけ `[LCtrl↓, キー↓]`／`[キー↑, LCtrl↑]` で包む。CapsLock↑の時点で Ctrl が離れているので解放が届く。Ctrl+クリックや Ctrl+ホイールは効かなくなる。ブリーフに無い機能で ADR が要る（未着手）
4. **`HKLM\...\Keyboard Layouts\00000411` の `Layout File` を `KBDUS.DLL` にする**（未検証）: `kbdus.dll` に NLS テーブルが無いので理屈の上では解放が届くはずだが、再起動が要るため未確認。`Alt + \`` と Ctrl/Shift/Alt + CapsLock の IME 切り替えは失う

## 7. 対応（2026-09-23）

- `examples/personal-ja.toml` と実運用の `personal-ja.toml` の `"CapsLock" = "LCtrl"` をコメントアウトし、理由をコメントに残した
- レジストリの Scancode Map を登録・解除する `.reg` を `examples/registry/` に追加した（オーナー指示 2026-09-23）
- README の制限事項への追記は未実施（提案: 「日本語キーボード種別では CapsLock の単キールールは使えない」。AGENTS.md 不変条件 5 と同じく、回避ハックではなく明文化で扱う）

## 8. 再現スクリプトの流し方

`winremap` のビルドは要らない（4.2〜4.4 は Windows 単体）。PowerShell 7 で、キー入力が飛んでよい窓（使い捨ての `cmd.exe` 等）を前面にして実行する。CapsLock のトグルと Ctrl が変わったままにならないよう、スクリプト末尾で元に戻す。

```powershell
# 単独タップ（正常）／Ctrl 押下中（解放が消える）
.\20260923_capslock-probe.ps1 -Seq "caps+,caps-"
.\20260923_capslock-probe.ps1 -Seq "ctrl+,caps+,caps-,ctrl-"
# 前面ウィンドウの入力言語を英語 (米国) にして同じ列（終了時に日本語へ戻す）
.\20260923_capslock-probe.ps1 -Seq "ctrl+,caps+,caps-,ctrl-" -Layout 00000409
# Raw Input 併用
.\20260923_capslock-rawinput-probe.ps1 -Seq "ctrl+,caps+,caps-,ctrl-"
```

4.1 を再現するには `cargo build --release --features test-inject` のバイナリを `--accept-injected --debug` で起動しておく（Store 版は先に止める。単一インスタンスの mutex が競合する）。`target/release/winremap.exe` はテスト後に `test-inject` 無しで再ビルドして戻すこと（AGENTS.md: リリース成果物に含めない）。
