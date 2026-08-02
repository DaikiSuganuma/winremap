# Checks the IME cursor tint (ADR 0067) on this machine, end to end.
#
# Reads the *system* arrow cursor's pixels rather than a screenshot: that is
# what SetSystemCursor replaces, and it answers the questions the design
# actually makes claims about — does it tint, does it come back, does a kill
# leave the tint behind (the signal that WinRemap died), and does the next
# start clear it.
#
# Needs a build with `--features test-inject`, and runs it with
# `--accept-injected`: a shipped build passes injected keys straight through
# (AGENTS.md invariant 1), so the 半角/全角 sent from here would never reach
# the indicator's key check the way a real keypress does (ADR 0053).
#
#   cargo build --release --features test-inject
#   .\tests\acceptance\probe-ime-cursor.ps1
#
# Also needs a Japanese IME on the machine, and WinRemap not already running
# (the single-instance mutex would stop the copy under test).
[CmdletBinding()]
param(
    [string]$Exe = ".\target\release\winremap.exe",
    # Deliberately not the default orange: a colour nothing else produces.
    [string]$Color = "#00a0ff",
    # Internal: re-invokes this script to take one measurement in a process
    # of its own (see Get-ArrowTint).
    [switch]$ReadArrowOnly
)

$ErrorActionPreference = 'Stop'
$pass = 0
$fail = 0

function Check([string]$name, [bool]$ok, [string]$detail) {
    $verdict = if ($ok) { "pass"; $script:pass++ } else { "FAIL"; $script:fail++ }
    "{0,-38} {1}  {2}" -f $name, $verdict, $detail
}

Add-Type -Namespace Probe -Name Cur -MemberDefinition @'
[DllImport("user32.dll", SetLastError=true)] public static extern IntPtr LoadCursorW(IntPtr h, int id);
[DllImport("user32.dll", SetLastError=true)] public static extern bool GetIconInfo(IntPtr icon, out INFO info);
[DllImport("gdi32.dll", SetLastError=true)] public static extern int GetObjectW(IntPtr h, int c, out BM bm);
[DllImport("gdi32.dll", SetLastError=true)] public static extern int GetDIBits(IntPtr dc, IntPtr bmp, uint start, uint lines, byte[] bits, ref BMI bmi, uint usage);
[DllImport("gdi32.dll", SetLastError=true)] public static extern IntPtr CreateCompatibleDC(IntPtr dc);
[DllImport("gdi32.dll", SetLastError=true)] public static extern bool DeleteDC(IntPtr dc);
[DllImport("gdi32.dll", SetLastError=true)] public static extern bool DeleteObject(IntPtr o);
[DllImport("user32.dll", SetLastError=true)] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);
[DllImport("user32.dll", SetLastError=true)] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll", SetLastError=true)] public static extern IntPtr GetForegroundWindow();
[DllImport("imm32.dll", SetLastError=true)] public static extern IntPtr ImmGetDefaultIMEWnd(IntPtr h);
[DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SendMessageW(IntPtr h, uint msg, IntPtr wp, IntPtr lp);
[StructLayout(LayoutKind.Sequential)] public struct INFO { public bool fIcon; public int xHot, yHot; public IntPtr hbmMask, hbmColor; }
[StructLayout(LayoutKind.Sequential)] public struct BM { public int type, w, h, wbytes; public short planes, bpp; public IntPtr bits; }
[StructLayout(LayoutKind.Sequential)] public struct BMI { public int size, w, h; public short planes, bpp; public int compression, imgSize, xppm, yppm, used, important; public int pad1, pad2, pad3; }
'@

# The arrow as the session currently has it, summarised as "how many pixels
# lean blue" — a black-and-white cursor has none.
function Read-ArrowTint {
    $cur = [Probe.Cur]::LoadCursorW([IntPtr]::Zero, 32512)  # IDC_ARROW
    $info = New-Object Probe.Cur+INFO
    if (-not [Probe.Cur]::GetIconInfo($cur, [ref]$info)) { return 0 }
    $source = if ($info.hbmColor -ne [IntPtr]::Zero) { $info.hbmColor } else { $info.hbmMask }
    $bm = New-Object Probe.Cur+BM
    [void][Probe.Cur]::GetObjectW($source, [System.Runtime.InteropServices.Marshal]::SizeOf($bm), [ref]$bm)
    $bmi = New-Object Probe.Cur+BMI
    $bmi.size = 40; $bmi.w = $bm.w; $bmi.h = -$bm.h; $bmi.planes = 1; $bmi.bpp = 32
    $bytes = New-Object byte[] ($bm.w * $bm.h * 4)
    $dc = [Probe.Cur]::CreateCompatibleDC([IntPtr]::Zero)
    [void][Probe.Cur]::GetDIBits($dc, $source, 0, $bm.h, $bytes, [ref]$bmi, 0)
    [void][Probe.Cur]::DeleteDC($dc)
    [void][Probe.Cur]::DeleteObject($info.hbmMask)
    if ($info.hbmColor -ne [IntPtr]::Zero) { [void][Probe.Cur]::DeleteObject($info.hbmColor) }
    $blue = 0
    # BGRA. "Leans blue" = clearly more blue than red, which no grey pixel of
    # a black-and-white cursor ever is.
    for ($i = 0; $i -lt $bytes.Length; $i += 4) { if ($bytes[$i] -gt $bytes[$i + 2] + 40) { $blue++ } }
    return $blue
}

if ($ReadArrowOnly) {
    Read-ArrowTint
    exit 0
}

# Every measurement in a process of its own. `LoadCursor` hands out a handle
# that stays cached for the life of the process, while a restore installs
# *new* cursor objects — so a second reading in the same process keeps
# showing the cursor that was replaced, and a working restore reads as a
# broken one. (Cost an hour of chasing the wrong bug on 2026-08-02.)
$selfPath = $PSCommandPath
$hostExe = (Get-Process -Id $PID).Path
function Get-ArrowTint {
    [int](& $hostExe -NoProfile -File $selfPath -ReadArrowOnly | Select-Object -Last 1)
}

# Puts the focused window's IME into a known state and gives WinRemap a
# reason to look, then returns the state the IME reports afterwards.
#
# The state is set through the IME window (`IMC_SETOPENSTATUS`) rather than
# by pressing 半角/全角: an injected toggle key does not move this machine's
# IME at all (measured 2026-08-02 — it stayed on across four presses), so a
# probe built on it would be measuring nothing. The keypress is still sent,
# for its other job: WinRemap re-reads the IME on a toggle-candidate key or a
# focus change, and nothing else.
function Set-Ime([int]$open) {
    $ime = [Probe.Cur]::ImmGetDefaultIMEWnd([Probe.Cur]::GetForegroundWindow())
    if ($ime -eq [IntPtr]::Zero) { throw "no IME window for the focused window" }
    [void][Probe.Cur]::SendMessageW($ime, 0x283, [IntPtr]6, [IntPtr]$open)   # IMC_SETOPENSTATUS
    [Probe.Cur]::keybd_event(0x19, 0, 0, [IntPtr]::Zero)                     # VK_KANJI, the poke
    Start-Sleep -Milliseconds 60
    [Probe.Cur]::keybd_event(0x19, 0, 2, [IntPtr]::Zero)
    # The poke arms a 50 ms timer, then the IME is queried across processes.
    Start-Sleep -Milliseconds 1500
    return [int][Probe.Cur]::SendMessageW($ime, 0x283, [IntPtr]5, [IntPtr]0) # IMC_GETOPENSTATUS
}

function Start-WinRemap([string]$config) {
    $p = Start-Process $Exe -ArgumentList '--config', $config, '--lang', 'en', '--accept-injected' -PassThru
    Start-Sleep -Seconds 3
    return $p
}

if (Get-Process winremap -ErrorAction SilentlyContinue) {
    throw "WinRemap is already running; exit it from the tray first."
}

$config = Join-Path ([IO.Path]::GetTempPath()) "winremap-cursor-probe.toml"
@"
[ime_indicator]
cursor = true
cursor_color = "$Color"

[[keymap]]
name = "probe"
application = ["*"]

[keymap.remap]
"C-F24" = "F24"
"@ | Set-Content $config -Encoding utf8

"before anything: $(Get-ArrowTint) blue-leaning pixels in the arrow"

$app = Start-WinRemap $config
Check "startup-clears-any-leftover" ((Get-ArrowTint) -eq 0) "a tint from a previous run must not survive the next start"

$notepad = Start-Process notepad -PassThru
Start-Sleep -Seconds 3
[void][Probe.Cur]::SetForegroundWindow($notepad.MainWindowHandle)
Start-Sleep -Milliseconds 700

$state = Set-Ime 1
$on = Get-ArrowTint
Check "tints-while-the-ime-is-on" ($state -eq 1 -and $on -gt 0) "IME reports $state; blue-leaning pixels: $on"

$state = Set-Ime 0
$off = Get-ArrowTint
Check "restores-when-the-ime-goes-off" ($state -eq 0 -and $off -eq 0) "IME reports $state; blue-leaning pixels: $off"

# The design's headline claim: killed while tinted, the tint stays. That is
# what makes "tinted, and no WinRemap in the tray" mean "it died" (ADR 0067).
[void](Set-Ime 1)
$beforeKill = Get-ArrowTint
Stop-Process -Id $app.Id -Force
Start-Sleep -Seconds 1
$afterKill = Get-ArrowTint
Check "a-kill-leaves-the-tint-behind" ($beforeKill -gt 0 -and $afterKill -gt 0) "before $beforeKill, after $afterKill"

# ...and the next start clears it, without anything having been recorded.
$app = Start-WinRemap $config
$afterRestart = Get-ArrowTint
Check "the-next-start-restores-it" ($afterRestart -eq 0) "blue-leaning pixels: $afterRestart"

Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
# The IME was left on in that window; put it back before closing.
[void][Probe.Cur]::SetForegroundWindow($notepad.MainWindowHandle)
Start-Sleep -Milliseconds 500
[void](Set-Ime 0)
Stop-Process -Id $notepad.Id -Force -ErrorAction SilentlyContinue
Remove-Item $config -ErrorAction SilentlyContinue

""
"RESULT: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
