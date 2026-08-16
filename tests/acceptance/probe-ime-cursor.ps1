# Checks the IME cursor tint (ADR 0067) on this machine, end to end.
#
# Reads the *system* cursors' pixels rather than a screenshot: that is what
# SetSystemCursor replaces, and it answers the questions the design actually
# makes claims about — does it tint, does it come back, does a kill leave the
# tint behind (the signal that WinRemap died), and does the next start clear
# it.
#
# Both cursors are asked in both directions. Until 2026-08-16 every check on
# the *restore* side read the arrow alone, which is the cursor that never had
# the problem (see Wait-BeamRestored).
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
    # A colour nothing else on the screen produces, so "leans blue" below can
    # only be this tint.
    [string]$Color = "#00a0ff",
    # Internal: re-invokes this script to take one measurement in a process
    # of its own (see Get-ArrowTint).
    [switch]$ReadArrowOnly,
    # Which system cursor to measure: 32512 = arrow, 32513 = I-beam.
    [int]$CursorId = 32512,
    # Run WinRemap with --debug into this file. For working out *why* a check
    # failed: the log says what WinRemap thought the IME was doing.
    [string]$DebugLog
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
[DllImport("user32.dll", SetLastError=true)] public static extern bool ShowWindow(IntPtr h, int cmd);
[DllImport("user32.dll", SetLastError=true)] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", SetLastError=true)] public static extern bool BringWindowToTop(IntPtr h);
[DllImport("user32.dll", SetLastError=true)] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr pid);
[DllImport("user32.dll", SetLastError=true, EntryPoint="GetWindowThreadProcessId")] public static extern uint GetWindowPid(IntPtr h, out uint owner);
[DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern int GetWindowTextW(IntPtr h, System.Text.StringBuilder text, int max);
[DllImport("user32.dll", SetLastError=true)] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
[DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
[DllImport("imm32.dll", SetLastError=true)] public static extern IntPtr ImmGetDefaultIMEWnd(IntPtr h);
[DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SendMessageW(IntPtr h, uint msg, IntPtr wp, IntPtr lp);
[DllImport("user32.dll", SetLastError=true)] public static extern bool GetCursorInfo(ref CURSORINFO info);
[DllImport("user32.dll", SetLastError=true)] public static extern bool SetCursorPos(int x, int y);
[DllImport("user32.dll", SetLastError=true)] public static extern bool GetClientRect(IntPtr h, out RECT r);
[DllImport("user32.dll", SetLastError=true)] public static extern bool ClientToScreen(IntPtr h, ref PT p);
[StructLayout(LayoutKind.Sequential)] public struct CURSORINFO { public int size, flags; public IntPtr cursor; public int x, y; }
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int left, top, right, bottom; }
[StructLayout(LayoutKind.Sequential)] public struct PT { public int x, y; }
[StructLayout(LayoutKind.Sequential)] public struct INFO { public bool fIcon; public int xHot, yHot; public IntPtr hbmMask, hbmColor; }
[StructLayout(LayoutKind.Sequential)] public struct BM { public int type, w, h, wbytes; public short planes, bpp; public IntPtr bits; }
[StructLayout(LayoutKind.Sequential)] public struct BMI { public int size, w, h; public short planes, bpp; public int compression, imgSize, xppm, yppm, used, important; public int pad1, pad2, pad3; }
'@

# Any bitmap as top-down 32-bit BGRA, whatever it is stored as. A monochrome
# one comes back black and white, which is what makes counting the AND mask
# below the same job as counting pixels.
function Read-Bitmap([IntPtr]$bitmap) {
    $bm = New-Object Probe.Cur+BM
    [void][Probe.Cur]::GetObjectW($bitmap, [System.Runtime.InteropServices.Marshal]::SizeOf($bm), [ref]$bm)
    $bmi = New-Object Probe.Cur+BMI
    $bmi.size = 40; $bmi.w = $bm.w; $bmi.h = -$bm.h; $bmi.planes = 1; $bmi.bpp = 32
    $bytes = New-Object byte[] ($bm.w * $bm.h * 4)
    $dc = [Probe.Cur]::CreateCompatibleDC([IntPtr]::Zero)
    [void][Probe.Cur]::GetDIBits($dc, $bitmap, 0, $bm.h, $bytes, [ref]$bmi, 0)
    [void][Probe.Cur]::DeleteDC($dc)
    return $bytes
}

# How many pixels of a cursor would actually be drawn.
#
# Counting differs by cursor kind. A modern cursor is drawn where its alpha
# is not zero. The stock I-beam has no colour bitmap and no solid pixels at
# all: it is visible only because it *inverts* the screen under it (AND=1
# with XOR=1), which is why "count the opaque pixels" reports zero for a
# perfectly visible cursor. A mask-only cursor's bitmap is twice as tall, the
# AND rows above the XOR rows.
#
# Zero is what M-2 was made of, and it is also what a broken *restore* leaves
# behind (ADR 0076): a cursor Windows is perfectly happy to hand out and draw
# nothing of. Measured on this machine on 2026-08-16, with 0.9.0 running: the
# registered I-beam was a 32x32 colour cursor with 0 drawn pixels.
function Measure-DrawnBits([byte[]]$bytes, [bool]$colored) {
    $drawn = 0
    if ($colored) {
        for ($i = 0; $i -lt $bytes.Length; $i += 4) { if ($bytes[$i + 3] -ne 0) { $drawn++ } }
        return $drawn
    }
    $half = ($bytes.Length / 4) / 2
    for ($i = 0; $i -lt $half; $i++) {
        $and = $bytes[$i * 4] -ne 0
        $xor = $bytes[($i + $half) * 4] -ne 0
        if (-not $and -or $xor) { $drawn++ }
    }
    return $drawn
}

# One system cursor as the session currently has it, summarised as
# "<blue-leaning> <alpha-transparent> <pixels read> <mask-transparent>
# <white> <drawn>".
#
# A black-and-white cursor leans nowhere, so "leans blue" can only be the
# tint. The two transparency counts are the same shape described twice — once
# in the colour bitmap's alpha channel and once in the 1-bit AND mask — and
# Windows does not always read the same one, so both have to say it (see the
# check that compares them).
function Read-ArrowTint {
    $cur = [Probe.Cur]::LoadCursorW([IntPtr]::Zero, $CursorId)
    $info = New-Object Probe.Cur+INFO
    if (-not [Probe.Cur]::GetIconInfo($cur, [ref]$info)) { return 0 }
    $colored = $info.hbmColor -ne [IntPtr]::Zero
    $bytes = Read-Bitmap $(if ($colored) { $info.hbmColor } else { $info.hbmMask })
    $blue = 0
    $clear = 0
    $white = 0
    # BGRA. "Leans blue" = clearly more blue than red, which no grey pixel of
    # a black-and-white cursor ever is. "White" = opaque and bright in all
    # three channels, which is the synthesised border and nothing else once
    # the body has taken the colour.
    for ($i = 0; $i -lt $bytes.Length; $i += 4) {
        if ($bytes[$i] -gt $bytes[$i + 2] + 40) { $blue++ }
        if ($bytes[$i + 3] -eq 0) { $clear++ }
        if ($bytes[$i + 3] -eq 255 -and $bytes[$i] -gt 215 -and $bytes[$i + 1] -gt 215 -and $bytes[$i + 2] -gt 215) { $white++ }
    }
    # The AND mask, where 1 (white, once read as 32-bit) means the screen
    # shows through. Only meaningful next to a colour bitmap: on a mask-only
    # cursor this bitmap is twice as tall and carries the XOR rows too.
    $masked = 0
    if ($colored) {
        $mask = Read-Bitmap $info.hbmMask
        for ($i = 0; $i -lt $mask.Length; $i += 4) {
            if ($mask[$i] -ne 0 -or $mask[$i + 1] -ne 0 -or $mask[$i + 2] -ne 0) { $masked++ }
        }
    }
    # And whether there is a cursor there at all. The four numbers above all
    # describe a *tint*, so an empty cursor satisfies every one of them by
    # having nothing to say — which is how "restored" came to include a state
    # with nothing in it.
    $drawn = Measure-DrawnBits $bytes $colored
    [void][Probe.Cur]::DeleteObject($info.hbmMask)
    if ($colored) { [void][Probe.Cur]::DeleteObject($info.hbmColor) }
    return "$blue $clear $($bytes.Length / 4) $masked $white $drawn"
}

# What is on the pointer *right now*, over the text of a real window.
#
# Everything above reads the cursor the system has registered, which is what
# SetSystemCursor writes — but not necessarily what the user sees. On
# 2026-08-08 the tinted I-beam went invisible on screen (acceptance M-2)
# while every registered-cursor measurement passed, so the two questions came
# apart and this one had no answer at all. GetCursorInfo hands out the cursor
# being displayed; a cursor with no drawn pixels is one the user sees nothing
# of, whatever the registry and the system cursor table say.
function Measure-Drawn([IntPtr]$window) {
    $r = New-Object Probe.Cur+RECT
    if (-not [Probe.Cur]::GetClientRect($window, [ref]$r)) { return $null }
    $p = New-Object Probe.Cur+PT
    $p.x = [int](($r.right - $r.left) * 0.5)
    $p.y = [int](($r.bottom - $r.top) * 0.6)
    [void][Probe.Cur]::ClientToScreen($window, [ref]$p)
    [void][Probe.Cur]::SetCursorPos($p.x, $p.y)
    Start-Sleep -Milliseconds 300

    $ci = New-Object Probe.Cur+CURSORINFO
    $ci.size = [System.Runtime.InteropServices.Marshal]::SizeOf($ci)
    if (-not [Probe.Cur]::GetCursorInfo([ref]$ci)) { return $null }
    if (($ci.flags -band 1) -eq 0) { return [pscustomobject]@{ Kind = 'hidden'; Visible = 0; Blue = 0 } }
    $info = New-Object Probe.Cur+INFO
    if (-not [Probe.Cur]::GetIconInfo($ci.cursor, [ref]$info)) { return $null }

    $colored = $info.hbmColor -ne [IntPtr]::Zero
    $bytes = Read-Bitmap $(if ($colored) { $info.hbmColor } else { $info.hbmMask })
    $visible = Measure-DrawnBits $bytes $colored
    # Only a colour cursor can lean anywhere; a mask-only one is the shape the
    # tint would have replaced.
    $blue = 0
    if ($colored) {
        for ($i = 0; $i -lt $bytes.Length; $i += 4) {
            if ($bytes[$i] -gt $bytes[$i + 2] + 40) { $blue++ }
        }
    }
    [void][Probe.Cur]::DeleteObject($info.hbmMask)
    if ($colored) { [void][Probe.Cur]::DeleteObject($info.hbmColor) }
    [pscustomobject]@{ Kind = $(if ($colored) { 'colour' } else { 'mask-only' }); Visible = $visible; Blue = $blue }
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
function Measure-Cursor([int]$id = 32512) {
    $raw = (& $hostExe -NoProfile -File $selfPath -ReadArrowOnly -CursorId $id | Select-Object -Last 1) -split ' '
    return [pscustomobject]@{ Blue = [int]$raw[0]; Clear = [int]$raw[1]; Total = [int]$raw[2]; Masked = [int]$raw[3]; White = [int]$raw[4]; Drawn = [int]$raw[5] }
}
function Get-ArrowTint {
    (Measure-Cursor 32512).Blue
}

# The cursor is changed by another process, reacting to something this script
# did, so at any single instant "not yet" and "never" look the same — a fixed
# sleep either makes the run slow or makes the check flaky, and this one was
# flaky (2 runs in 3 failed the restore, 2026-08-02). Poll for the expected
# answer instead: a pass costs one reading, and only a real failure costs the
# timeout. Each reading spawns a process, which paces the loop by itself.
function Wait-Tint([scriptblock]$until, [int]$timeoutMs = 8000) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ($true) {
        $tint = Get-ArrowTint
        if ((& $until $tint) -or (Get-Date) -gt $deadline) { return $tint }
    }
}

# The same wait, for the checks that would otherwise blame WinRemap for
# somebody else's window.
#
# The tint follows whatever window is in front — that is the feature. So this
# script owning the foreground is a precondition of every tint reading, not a
# detail, and it does not own the machine it runs on: on 2026-08-03 two runs
# recorded a product failure while the foreground had drifted away mid-wait
# (one of them to the desktop itself, which the indicator deliberately skips,
# ADR 0023). Set-Focus already fails loudly when the foreground cannot be
# *taken*; nothing noticed when it was taken *back*.
#
# A reading that satisfies the condition needs no defending. One that does not
# is worth recording only with the target still in front, so take it back and
# read once more before believing it.
function Wait-TintOwning([scriptblock]$until) {
    foreach ($attempt in 1..2) {
        $tint = Wait-Tint $until
        if (& $until $tint) { return $tint }
        if ([Probe.Cur]::GetForegroundWindow() -eq $script:target) { return $tint }
        Set-Focus $script:target
    }
    throw "the foreground kept leaving the window under test (now $(Get-FrontName)); a tint read there is about that window, not about WinRemap"
}

# The other half of every restore, and until 2026-08-16 nobody's job.
#
# Every "the tint came off" check waits on the arrow — and the arrow is the
# cursor that never had the problem. A restore that put the arrow back and
# left the I-beam tinted passed all of them; so did one that left an I-beam
# with **nothing drawn in it** registered, which is what a reload from the
# registry did on a scaled display until 1.0.0 (ADR 0076). Measured on this
# machine on 2026-08-16 with 0.9.0 running: a 32x32 colour I-beam, 0 drawn.
# That state is not "restored" under any reading, and nothing here could see
# it.
#
# So the I-beam is asked two questions after each restore, and they fail
# differently: the tint is gone (nothing leans blue), and there is still a
# cursor there (something is drawn).
#
# Polled for the same reason Wait-Tint is, and the wait is real: the arrow is
# restored first, so a reading taken the instant the arrow comes back can
# catch the I-beam mid-restore.
function Wait-BeamRestored([int]$timeoutMs = 4000) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ($true) {
        $beam = Measure-Cursor 32513
        if ((Test-BeamRestored $beam) -or (Get-Date) -gt $deadline) { return $beam }
    }
}

function Test-BeamRestored($beam) {
    return $beam.Blue -eq 0 -and $beam.Drawn -gt 0
}

function Format-Beam($beam) {
    return "I-beam: blue-leaning $($beam.Blue), drawn $($beam.Drawn)"
}

# The two windows this drives. The IME's open state is **per window**, and
# WinRemap reads whichever window is in front, so a probe that lets the
# foreground drift is measuring an unrelated window's IME — two runs failed
# different checks that way (2026-08-02). `other` exists only to be switched
# to and away from; see Set-Ime.
$script:target = [IntPtr]::Zero
$script:other = [IntPtr]::Zero

function Get-ImeWnd {
    $ime = [Probe.Cur]::ImmGetDefaultIMEWnd([Probe.Cur]::GetForegroundWindow())
    if ($ime -eq [IntPtr]::Zero) { throw "no IME window for the focused window" }
    return $ime
}

function Read-Ime([IntPtr]$ime) {
    return [int][Probe.Cur]::SendMessageW($ime, 0x283, [IntPtr]5, [IntPtr]0)  # IMC_GETOPENSTATUS
}

# What is in front, in words, with the IME state that goes with it.
#
# This script does not own the machine it runs on. Set-Focus fails loudly when
# it cannot *take* the foreground, but nothing noticed when the foreground was
# taken *back* — and a tint check that fails with nothing but a pixel count
# cannot tell "WinRemap is wrong" from "the desktop moved". That is not
# hypothetical: startup-clears-any-leftover failed once on 2026-08-03, on the
# first run after the VM suite, and left no record of what it had been looking
# at; five consecutive runs afterwards passed and the cause is still unknown.
function Get-FrontName {
    $front = [Probe.Cur]::GetForegroundWindow()
    $title = New-Object System.Text.StringBuilder 256
    [void][Probe.Cur]::GetWindowTextW($front, $title, $title.Capacity)
    $owner = 0
    [void][Probe.Cur]::GetWindowPid($front, [ref]$owner)
    $name = (Get-Process -Id $owner -ErrorAction SilentlyContinue).ProcessName
    $ime = try { Read-Ime (Get-ImeWnd) } catch { '?' }
    return "$name '$($title.ToString())' ime=$ime"
}

# Brings a window to the front, and **fails loudly if it cannot**.
#
# `SetForegroundWindow` is refused outright for a process that is not itself
# in the foreground and has not just received input — so a script run from a
# background session is simply ignored, and every reading afterwards is of
# whatever window happened to be there instead. That is not a hypothetical:
# it silently produced 3-passed-5-failed runs whose real cause was that the
# foreground never moved at all (2026-08-02, found by reading WinRemap's own
# debug log, which had one `[window]` line for the whole run).
#
# Attaching to the foreground window's input queue lifts the restriction for
# the duration — the documented way in, and the reason the earlier
# keypress-driven version of this script appeared to work: injecting a key
# also grants the right, as a side effect.
function Set-Focus([IntPtr]$window) {
    foreach ($attempt in 1..5) {
        $front = [Probe.Cur]::GetForegroundWindow()
        if ($front -eq $window) { Start-Sleep -Milliseconds 250; return }
        $me = [Probe.Cur]::GetCurrentThreadId()
        $owner = [Probe.Cur]::GetWindowThreadProcessId($front, [IntPtr]::Zero)
        $attached = $owner -ne 0 -and $owner -ne $me -and [Probe.Cur]::AttachThreadInput($me, $owner, $true)
        [void][Probe.Cur]::BringWindowToTop($window)
        [void][Probe.Cur]::SetForegroundWindow($window)
        if ($attached) { [void][Probe.Cur]::AttachThreadInput($me, $owner, $false) }
        Start-Sleep -Milliseconds 300
    }
    if ([Probe.Cur]::GetForegroundWindow() -ne $window) {
        throw "could not bring window $window to the front; every reading after this would be of some other window's IME"
    }
}

# Sets the state without telling WinRemap. For arranging the ground before a
# start, when there is no WinRemap running to notice.
function Set-ImeDirect([int]$open) {
    Set-Focus $script:target
    $ime = Get-ImeWnd
    [void][Probe.Cur]::SendMessageW($ime, 0x283, [IntPtr]6, [IntPtr]$open)
    # Read it back instead of sleeping on it. This was the last place the script
    # arranged state without measuring the result, and an arrangement that
    # quietly did not take makes the check after it a coin toss — the trap
    # Set-Focus was hardened against on 2026-08-02, left standing here.
    $deadline = (Get-Date).AddMilliseconds(2000)
    while ((Read-Ime $ime) -ne $open -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 25 }
    $got = Read-Ime $ime
    if ($got -ne $open) { throw "the IME would not go to $open (reads $got); in front: $(Get-FrontName)" }
}

# Puts the target window's IME into a known state and gives WinRemap a reason
# to look, then returns the state the IME reports afterwards.
#
# Two things have to happen: the IME must end up in the wanted state, and
# WinRemap must have a reason to re-read it — it only looks on a
# toggle-candidate key or a foreground change, nothing else.
#
# **The trigger is the foreground change, not a keypress.** Sending 半角/全角
# was tried first and does not hold: whether an injected toggle key also moves
# this machine's IME changed between runs on one machine in one afternoon
# (2026-08-02 — no effect across four presses in the morning, a reliable
# toggle later, and back again). Measuring that once per run and aiming around
# it still failed, because it can differ *within* a run. The key path is what
# the owner uses in real life; it is not what this measures.
#
# It takes a **second window**, and minimising the first one is not a
# substitute: WinRemap keys off `EVENT_SYSTEM_FOREGROUND` and skips a window
# it is already on, so minimise-and-restore produces no re-read at all
# (measured 2026-08-02 — the debug log showed one `[window]` line for the
# whole run). Windows 11's Notepad opens a *tab* on the second launch and
# hands back no window handle, so the second window is a console window.
function Set-Ime([int]$open) {
    Set-Focus $script:target
    $ime = Get-ImeWnd
    [void][Probe.Cur]::SendMessageW($ime, 0x283, [IntPtr]6, [IntPtr]$open)    # IMC_SETOPENSTATUS
    Start-Sleep -Milliseconds 150
    Set-Focus $script:other
    Set-Focus $script:target
    # The foreground change arms a short timer, then the IME is queried across
    # processes.
    Start-Sleep -Milliseconds 800
    return Read-Ime $ime
}

# Windows 11's Notepad is a packaged application: the process that gets
# started can hand the work to one that is already running and then exit, so
# the window is not always owned by the process `Start-Process` handed back —
# and it is not there the instant the call returns either. Wait for a window,
# from the launched process if it has one and from any process of that name
# if it does not.
function Wait-MainWindow($process, [string]$name, [int]$timeoutMs = 15000) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ((Get-Date) -lt $deadline) {
        if (-not $process.HasExited) {
            $process.Refresh()
            if ($process.MainWindowHandle -ne [IntPtr]::Zero) { return $process.MainWindowHandle }
        }
        $owner = Get-Process $name -ErrorAction SilentlyContinue |
            Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | Select-Object -First 1
        if ($owner) { return $owner.MainWindowHandle }
        Start-Sleep -Milliseconds 500
    }
    throw "$name did not give up a window handle within $timeoutMs ms"
}

function Start-WinRemap([string]$config) {
    $arguments = @('--config', $config, '--lang', 'en', '--accept-injected')
    # Redirected, so WinRemap writes to the file rather than opening its own
    # console (ADR 0068) — which would also steal the foreground and change
    # the very thing being measured.
    if ($DebugLog) { $arguments += '--debug' }
    $p = if ($DebugLog) {
        Start-Process $Exe -ArgumentList $arguments -PassThru -RedirectStandardOutput $DebugLog
    } else {
        Start-Process $Exe -ArgumentList $arguments -PassThru
    }
    Start-Sleep -Seconds 3
    return $p
}

if (Get-Process winremap -ErrorAction SilentlyContinue) {
    throw "WinRemap is already running; exit it from the tray first."
}

$config = Join-Path ([IO.Path]::GetTempPath()) "winremap-cursor-probe.toml"
@"
[ime_indicator]
change_cursor_color = true
cursor_color = "$Color"

[[keymap]]
name = "probe"
application = ["*"]

[keymap.remap]
"C-F24" = "F24"
"@ | Set-Content $config -Encoding utf8

"before anything: $(Get-ArrowTint) blue-leaning pixels in the arrow"

# The window under test comes first, and the IME goes off before WinRemap
# starts: "a start clears the tint" is only a statement about the tint if the
# IME is not on at the time, or WinRemap is right to put the colour straight
# back.
$notepad = Start-Process notepad -PassThru
$script:target = Wait-MainWindow $notepad 'notepad'
# Something to switch away to, so that switching back is a foreground change.
# `winver` because it is a plain Win32 window that is always present and does
# nothing: a console window reports no handle here, and an Explorer window
# would be a shell surface, which the indicator deliberately skips (ADR 0023).
$other = Start-Process winver -PassThru
$script:other = Wait-MainWindow $other 'winver'
Set-ImeDirect 0

$front = Get-FrontName
$app = Start-WinRemap $config
$leftover = Wait-TintOwning { param($t) $t -eq 0 }
$beamStart = Wait-BeamRestored
Check "startup-clears-any-leftover" ($leftover -eq 0 -and (Test-BeamRestored $beamStart)) "blue-leaning pixels: $leftover; $(Format-Beam $beamStart); in front at the start: $front; after: $(Get-FrontName)"

$state = Set-Ime 1
$on = Wait-TintOwning { param($t) $t -gt 0 }
Check "tints-while-the-ime-is-on" ($state -eq 1 -and $on -gt 0) "IME reports $state; blue-leaning pixels: $on; in front: $(Get-FrontName)"

# The I-beam is a mask-only cursor, and getting one wrong does not look like
# a missing tint — it looks like a black square over the text (owner report,
# 2026-08-02). An I-beam is a thin shape in a mostly empty box, so "the shape
# survived" is: some pixels took the colour, and most of the box is still
# transparent. The untouched cursor is no use as a baseline here — mask-only
# cursors carry no alpha to compare against, which is the whole problem.
$beam = Measure-Cursor 32513
$kept = $beam.Blue -gt 0 -and $beam.Clear -gt $beam.Total / 2
Check "the-i-beam-keeps-its-shape" $kept "blue $($beam.Blue), transparent $($beam.Clear) of $($beam.Total)"

# Being transparent in the alpha channel is not enough: Windows has more than
# one path for putting a cursor on the screen, and the one that reads the
# 1-bit AND mask draws a solid rectangle whenever that mask says "opaque
# everywhere". Zed took that path (owner report, 2026-08-02). So the shape has
# to be stated twice, and the two statements have to agree.
$arrow = Measure-Cursor 32512
$agree = $arrow.Masked -eq $arrow.Clear -and $beam.Masked -eq $beam.Clear
Check "the-shape-is-in-the-mask-too" $agree "arrow mask $($arrow.Masked) vs alpha $($arrow.Clear); I-beam mask $($beam.Masked) vs alpha $($beam.Clear)"

# The tint alone is invisible on a dark application: #0078d4 is 37% as bright
# as the white body it replaces (owner report, 2026-08-02). The white border
# is what makes the cursor two colours at opposite ends of the brightness
# range, so one of them stands out whatever is underneath.
$bordered = $arrow.White -gt 0 -and $beam.White -gt 0
Check "there-is-a-white-border" $bordered "arrow $($arrow.White) white pixels, I-beam $($beam.White)"

# And the same question asked of the screen rather than of the system cursor
# table: with the pointer over Notepad's text and the IME on, is there a
# tinted I-beam actually being drawn? Acceptance M-2 is exactly the case
# where the checks above pass and this one is what the owner is looking at.
$drawn = Measure-Drawn $script:target
$isDrawn = $null -ne $drawn -and $drawn.Visible -gt 0 -and $drawn.Blue -gt 0
Check "the-i-beam-on-screen-is-the-tinted-one" $isDrawn $(
    if ($null -eq $drawn) { "could not read the displayed cursor" }
    else { "$($drawn.Kind), drawn pixels $($drawn.Visible), blue-leaning $($drawn.Blue)" })

$state = Set-Ime 0
$off = Wait-TintOwning { param($t) $t -eq 0 }
$beamOff = Wait-BeamRestored
Check "restores-when-the-ime-goes-off" ($state -eq 0 -and $off -eq 0 -and (Test-BeamRestored $beamOff)) "IME reports $state; blue-leaning pixels: $off; $(Format-Beam $beamOff); in front: $(Get-FrontName)"

# The same thing, over and over (ADR 0073 decision 6).
#
# Every check above measures one transition, and M-2 was never a first
# transition: the tint went on correctly, the owner kept working, and some
# time later the I-beam was drawn with no pixels at all. Once that happened
# it stayed — `tinted` reads the cursor the session currently has, and an
# empty one recolours to another empty one, so every later toggle reproduced
# it. A single on-and-off cannot see either half of that. This can.
#
# Both questions are asked each round, because they fail differently: the
# system cursor table can hold a perfectly good tint while what is drawn over
# the text is empty (that is exactly what 2026-08-08 looked like).
$rounds = 3
$trouble = @()
foreach ($round in 1..$rounds) {
    [void](Set-Ime 1)
    [void](Wait-TintOwning { param($t) $t -gt 0 })
    $beam = Measure-Cursor 32513
    $drawn = Measure-Drawn $script:target
    $drawnPixels = if ($null -eq $drawn) { -1 } else { $drawn.Visible }
    if ($beam.Blue -le 0 -or $drawnPixels -le 0) {
        $trouble += "round ${round}: registered I-beam blue $($beam.Blue), drawn pixels $drawnPixels"
    }
    [void](Set-Ime 0)
    $off = Wait-TintOwning { param($t) $t -eq 0 }
    if ($off -ne 0) { $trouble += "round ${round}: $off blue-leaning pixels still there with the IME off" }
    # Both sides of the round, for the reason the round exists: a restore that
    # goes wrong on the tenth toggle looks exactly like one that never did.
    $beamOff = Wait-BeamRestored
    if (-not (Test-BeamRestored $beamOff)) { $trouble += "round ${round}: after the restore, $(Format-Beam $beamOff)" }
}
Check "repeated-toggles-never-empty-the-tint" ($trouble.Count -eq 0) $(
    if ($trouble.Count) { $trouble -join '; ' } else { "$rounds rounds, tinted and restored every time" })

# The design's headline claim: killed while tinted, the tint stays. That is
# what makes "tinted, and no WinRemap in the tray" mean "it died" (ADR 0067).
[void](Set-Ime 1)
$beforeKill = Get-ArrowTint
Stop-Process -Id $app.Id -Force
Start-Sleep -Seconds 1
$afterKill = Get-ArrowTint
Check "a-kill-leaves-the-tint-behind" ($beforeKill -gt 0 -and $afterKill -gt 0) "before $beforeKill, after $afterKill"

# ...and the next start clears it, without anything having been recorded.
# The IME goes off first, and directly: with the tint left behind and nothing
# running to notice, this is arranging the ground, not a thing being measured
# — and it keeps "the colour went away" from meaning "the IME went off".
Set-ImeDirect 0
$front = Get-FrontName
$app = Start-WinRemap $config
$afterRestart = Wait-TintOwning { param($t) $t -eq 0 }
$beamBack = Wait-BeamRestored
Check "the-next-start-restores-it" ($afterRestart -eq 0 -and (Test-BeamRestored $beamBack)) "blue-leaning pixels: $afterRestart; $(Format-Beam $beamBack); in front at the start: $front; after: $(Get-FrontName)"

Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Stop-Process -Id $notepad.Id, $other.Id -Force -ErrorAction SilentlyContinue
Remove-Item $config -ErrorAction SilentlyContinue

""
"RESULT: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
