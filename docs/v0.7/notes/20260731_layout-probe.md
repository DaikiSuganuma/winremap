# キーボード配列の実測（記号キー対応の Phase 0）

- 作成日: 2026-07-31
- 作成: Claude Code（AI モデル: claude-opus-5[1m]）／判断: オーナー
- 対象: [v0.7 開発計画 §2.2](../01_development-plan.md)
- 公式: [`VkKeyScanW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-vkkeyscanw)・[`MapVirtualKeyW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-mapvirtualkeyw)・[`GetKeyboardLayout`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getkeyboardlayout)・[Virtual-Key Codes](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes)

記号キー対応の設計を書く前に、**開発機で実際に何が返るか**を採った記録である。設計への帰結は開発計画 §2.2 にまとめてあり、ここには**再現手順と生の出力**を置く。JP 106 キーボードでの実測がまだ無いので、その機械が手に入ったときに同じものを流して追記する。

---

## 1. 流し方

`winremap` のビルドは要らない。PowerShell 5.1 / 7 のどちらでもよい。

```powershell
@'
using System;using System.Runtime.InteropServices;using System.Text;
public class Lay {
 [DllImport("user32.dll")] public static extern short VkKeyScanW(char ch);
 [DllImport("user32.dll")] public static extern uint MapVirtualKeyW(uint uCode,uint uMapType);
 [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint idThread);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetKeyNameTextW(int lParam,StringBuilder s,int n);
 public static string KeyName(uint vk){var sc=MapVirtualKeyW(vk,0);var sb=new StringBuilder(64);GetKeyNameTextW((int)(sc<<16),sb,64);return sb.ToString();}
}
'@ | ForEach-Object { Add-Type -TypeDefinition $_ }

"HKL = 0x{0:X}" -f [Lay]::GetKeyboardLayout(0)

$oem = @{0xBA='OEM_1';0xBB='OEM_PLUS';0xBC='OEM_COMMA';0xBD='OEM_MINUS';0xBE='OEM_PERIOD';
         0xBF='OEM_2';0xC0='OEM_3';0xDB='OEM_4';0xDC='OEM_5';0xDD='OEM_6';0xDE='OEM_7';
         0xDF='OEM_8';0xE2='OEM_102'}
foreach ($vk in ($oem.Keys | Sort-Object)) {
  $ch = [Lay]::MapVirtualKeyW([uint32]$vk, 2) -band 0xFFFF   # MAPVK_VK_TO_CHAR
  $c = if ($ch -gt 32) { [char]$ch } else { '(none)' }
  "0x{0:X2} {1,-10} {2,-16} {3}" -f $vk, $oem[$vk], $c, ([Lay]::KeyName([uint32]$vk))
}

foreach ($c in ';',':','@','[',']','\','^','-',',','.','/','_','+','*','=') {
  $r = [Lay]::VkKeyScanW([char]$c)
  if ($r -eq -1) { "{0} -> not on this layout" -f $c }
  else { "{0} -> vk 0x{1:X2} shiftstate {2}" -f $c, ($r -band 0xFF), (($r -shr 8) -band 0xFF) }
}
```

> `MapVirtualKeyW` の第 2 引数 `2` は `MAPVK_VK_TO_CHAR`。`VkKeyScanW` の戻り値は下位バイトが VK、上位バイトがシフト状態（1 = Shift、2 = Ctrl、4 = Alt）で、`-1` は**その配列にその文字が無い**という意味である。

---

## 2. 開発機の出力（2026-07-31）

環境: Windows 11 Pro 26200 / 入力言語は日本語 / **物理キーボードは US 配列**。

```
HKL = 0x4110411

VK   name        MAPVK_VK_TO_CHAR   GetKeyNameText
0xBA OEM_1       ;                  ;
0xBB OEM_PLUS    =                  =
0xBC OEM_COMMA   ,                  ,
0xBD OEM_MINUS   -                  -
0xBE OEM_PERIOD  .                  .
0xBF OEM_2       /                  /
0xC0 OEM_3       `                  `
0xDB OEM_4       [                  [
0xDC OEM_5       \                  \
0xDD OEM_6       ]                  ]
0xDE OEM_7       '                  '
0xDF OEM_8       (none)
0xE2 OEM_102     \                  \

; -> vk 0xBA shiftstate 0
: -> vk 0xBA shiftstate 1
@ -> vk 0x32 shiftstate 1
[ -> vk 0xDB shiftstate 0
] -> vk 0xDD shiftstate 0
\ -> vk 0xDC shiftstate 0
^ -> vk 0x36 shiftstate 1
- -> vk 0xBD shiftstate 0
, -> vk 0xBC shiftstate 0
. -> vk 0xBE shiftstate 0
/ -> vk 0xBF shiftstate 0
_ -> vk 0xBD shiftstate 1
+ -> vk 0xBB shiftstate 1
* -> vk 0x38 shiftstate 1
= -> vk 0xBB shiftstate 0
```

---

## 3. 読み取れること（4 点）

1. **`HKL` は 0x0411（日本語）なのに、刻印は US 配列である。** 入力言語 ID から物理配列は判定できない。日本語入力＋US キーボードは開発者には普通の構成で、オーナーの開発機がそれである。**レイアウト ID による分岐を書いてはならない**
2. **`@` はこの機械では OEM キーですらない。** `S-2`（vk 0x32・シフト面）である。JP 106 キーボードでは `0xC0` の非シフト面にある。同じ `"C-@"` が機械によって 2 通りに解決されうる
3. **`\` を返す VK が 2 つある**（`0xDC` と `0xE2`）。`VkKeyScanW` は片方しか返さない。JP 106 では `0xDC` が `¥`、`0xE2` が `\`（ろ）で別のキーである。**文字だけでは指せないキーが実在する**
4. **`0xDF`（OEM_8）は文字を持たない。** 名前でしか指せない

3 と 4 が、「刻印の文字だけ」という記法を採れない直接の理由である（開発計画 §2.3）。

---

## 4. JP 106 キーボードでの実測（未実施）

同じスクリプトを JP 106 キーボードを繋いだ状態で流し、下表を埋める。**受け入れ S-1〜S-3 の裏取りに使う。**

| VK | 期待（JP 106） | 実測 |
|---|---|---|
| `0xBA` OEM_1 | `:` | — |
| `0xBB` OEM_PLUS | `;` | — |
| `0xC0` OEM_3 | `@` | — |
| `0xDB` OEM_4 | `[` | — |
| `0xDC` OEM_5 | `¥` | — |
| `0xDD` OEM_6 | `]` | — |
| `0xDE` OEM_7 | `^` | — |
| `0xE2` OEM_102 | `\`（ろ） | — |

実機が用意できない場合は、**この期待値を偽テーブルとして単体テストに入れる**（開発計画 §2.4-4）。表を実装に持ち込むのではなく、テストの入力として持つ — 実装は常に Windows に聞く。
