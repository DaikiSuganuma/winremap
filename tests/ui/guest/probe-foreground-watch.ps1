# Runs inside the guest, in session 1. Diagnostic for the defect
# 06-foreground-line found: with a WinRemap window open, a switch to another
# application is neither reported on the foreground line nor picked up by the
# cache the hook reads, so a keymap scoped to that application stops applying.
#
# This script asserts nothing about whether that is acceptable. It exists to
# narrow down WHERE it breaks, by measuring the same switch four times under
# four conditions, with three independent records of what happened:
#
#   1. an independent EVENT_SYSTEM_FOREGROUND client in another process
#      (foreground-listener.ps1) - "did the system deliver the event at all"
#   2. WinRemap's own --debug transcript, redirected to a file so it is
#      readable whether or not a window is open - "did WinRemap act on it"
#   3. what Notepad holds after C-h - "did the keymap still apply"
#
# Record 1 is the one that cannot be obtained from inside WinRemap. Without
# it, "the system never sent it" and "WinRemap never handled it" are the same
# observation, and they have different fixes.
#
# The four stages, in order, on one long-lived WinRemap process:
#
#   S0  no WinRemap window          (the control - known good)
#   S1  the log window is open      (the state where it was seen to break)
#   S2  the log window closed again (does it recover?)
#   S3  the settings window is open (is it the log window, or any window?)
#
# Every stage makes the same switch between two third-party applications
# (Explorer -> Notepad), because that is the switch a user makes. Counting is
# by delta per stage rather than by timestamp arithmetic: each record is
# counted before and after, so nothing depends on three clocks agreeing.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932.
#
# Host side: .\run-vm-ui-test.ps1 -Check 90-probe-foreground

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\90-probe-foreground.txt"
$listenerOut = "C:\Test\watch-listener.txt"
$debugOut = "C:\Test\watch-debug.txt"

$lines = New-Object System.Collections.Generic.List[string]
$checks = [ordered]@{}
$total = [System.Diagnostics.Stopwatch]::StartNew()
function Say([string]$s) { $lines.Add($s) }
function Check([string]$name, $Ok, [string]$detail) {
    $ok = if ($Ok -is [bool]) { $Ok } else { @($Ok | Where-Object { $_ }).Count -gt 0 }
    $checks[$name] = $ok
    Say ("CHECK {0,-34} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}
function Flush { $lines | Set-Content $outPath -Encoding UTF8 }
trap { Say "EXCEPTION: $_"; Say $_.ScriptStackTrace; Flush; exit 1 }

. "$PSScriptRoot\ui-helpers.ps1"
. "$PSScriptRoot\winapp-helpers.ps1"

Say ("started " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))

# --- the three records ----------------------------------------------------

function Read-Utf8([string]$path) {
    if (-not (Test-Path $path)) { return , @() }
    try { return , @(Get-Content $path -Encoding UTF8 -ErrorAction Stop) } catch { return , @() }
}
# How many times each record has named Notepad so far. Deltas across a stage
# are what the stage means; absolute counts carry startup noise.
function Count-Listener { return @((Read-Utf8 $listenerOut) | Where-Object { $_ -match "notepad\.exe" }).Count }
function Count-Reports { return @((Read-Utf8 $debugOut) | Where-Object { $_ -match 'application = "notepad\.exe"' }).Count }

# --- the windows ----------------------------------------------------------

function Find-ByClass([string]$Class) {
    foreach ($w in $root.FindAll($kids, $any)) { if ($w.Current.ClassName -eq $Class) { return $w } }
    return $null
}
function Find-Notepad { return (Find-ByClass "Notepad") }
function Find-Explorer { return (Find-ByClass "CabinetWClass") }

function Get-PadText {
    $pad = Find-Notepad
    if (-not $pad) { return "" }
    foreach ($e in $pad.FindAll($desc, $any)) {
        if ("$($e.Current.ControlType.ProgrammaticName)" -notmatch "Document|Edit") { continue }
        try {
            $v = $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
            if ($v) { return ([string]$v).Trim() }
        }
        catch { }
    }
    return ""
}
function Wait-PadText([string]$Pattern, [int]$TimeoutMs = 4000) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $t = ""
    do {
        $t = Get-PadText
        if ($t -match $Pattern) { return $t }
        Start-Sleep -Milliseconds 300
    } while ($sw.ElapsedMilliseconds -lt $TimeoutMs)
    return $t
}

# One stage: leave Notepad, come back to it, then ask all three records what
# they saw. The trip through Explorer is what makes the return a change -
# focusing a window that is already in front fires nothing.
function Measure-Stage([string]$Name, [string]$State) {
    Say ""
    Say ("=== " + $Name + ": " + $State + " ===")
    $l0 = Count-Listener
    $r0 = Count-Reports
    # Where each record stood before the stage, so the stage can print exactly
    # the lines IT produced. The first run of this probe printed only counts
    # and left the raw records to the end - and then died before reaching
    # them, which is how a run that measured everything came back saying
    # nothing.
    $listener0 = @(Read-Utf8 $listenerOut).Count
    $debug0 = @(Read-Utf8 $debugOut).Count

    $ex = Find-Explorer
    $exOk = ([bool]$ex -and (Focus-Window $ex))
    Start-Sleep -Milliseconds 800
    $pad = Find-Notepad
    $padOk = ([bool]$pad -and (Focus-Window $pad))
    Start-Sleep -Milliseconds 1200

    # The consequence, measured the way a user would notice it: uitest.toml
    # maps C-h to x for notepad.exe only.
    [Nat]::Chord(0x11, 0x41)   # Ctrl+A
    Start-Sleep -Milliseconds 200
    [Nat]::Key(0x2E)           # Delete
    Start-Sleep -Milliseconds 300
    foreach ($vk in @(0x41, 0x42, 0x43)) { [Nat]::Key([byte]$vk) }
    Wait-PadText '^abc$' | Out-Null
    [Nat]::Chord(0x11, 0x48)   # C-h
    $text = Wait-PadText '^(abcx|xabc)$'

    Start-Sleep -Milliseconds 1200
    $l1 = Count-Listener
    $r1 = Count-Reports

    $row = [pscustomobject]@{
        Stage    = $Name
        State    = $State
        Focus    = ("explorer=" + $exOk + " notepad=" + $padOk)
        Listener = ($l1 - $l0)
        Reported = ($r1 - $r0)
        Text     = $text
        Remapped = ($text -match '^(abcx|xabc)$')
    }
    Say ("  focus:    " + $row.Focus)
    Say ("  listener saw notepad:   " + $row.Listener)
    Say ("  winremap reported it:   " + $row.Reported)
    Say ("  notepad text after C-h: '" + $text + "'  (remapped: " + $row.Remapped + ")")
    # The raw records for this stage only. What WinRemap logged while the keys
    # were arriving is the difference between "the hook never saw the key" and
    # "the hook saw it and found no rule", which no count can carry.
    $ls = @(Read-Utf8 $listenerOut)
    foreach ($l in @($ls | Select-Object -Skip $listener0)) { Say ("  L| " + $l) }
    $ds = @(Read-Utf8 $debugOut)
    foreach ($l in @($ds | Select-Object -Skip $debug0)) { Say ("  D| " + $l) }
    return $row
}

# --- set the scene --------------------------------------------------------

Say ""
Say "=== setting up ==="

# The listener first, so it is already pumping before anything moves.
Start-Process powershell.exe -ArgumentList @(
    "-NoProfile", "-ExecutionPolicy", "Bypass",
    "-File", "$PSScriptRoot\foreground-listener.ps1",
    "-OutPath", $listenerOut, "-Seconds", "300"
) -WindowStyle Minimized
Start-Sleep -Seconds 4
Say ("listener file exists: " + (Test-Path $listenerOut))

# WinRemap with --debug redirected to a file. This is what makes the four
# stages comparable: the log window's own contents are only readable while it
# is open, and two of the four stages have no log window.
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
Remove-Item $debugOut -Force -ErrorAction SilentlyContinue
Start-Process $exe -ArgumentList '--config', 'C:\Test\uitest.toml', '--lang', 'en', '--debug', '--accept-injected' `
    -NoNewWindow -RedirectStandardOutput $debugOut
Start-Sleep -Seconds 6
Check "app-running" ([bool](Get-Process winremap -ErrorAction SilentlyContinue)) "winremap.exe is resident with --debug"
# The banner, because two of this probe's premises are visible in it: that the
# binary is the test-inject build (it says TEST BUILD) and which config it
# read. A shipping build passes injected keys through, and every stage below
# would then show "not remapped" for a reason that has nothing to do with the
# defect under investigation.
foreach ($l in @((Read-Utf8 $debugOut) | Select-Object -First 8)) { Say ("  B| " + $l) }

Start-Process explorer.exe -ArgumentList "C:\Test"
Start-Sleep -Seconds 4
Start-Process notepad.exe
Start-Sleep -Seconds 5
Check "both-apps-are-up" ([bool](Find-Explorer) -and [bool](Find-Notepad)) "an Explorer window and a Notepad window to switch between"

# --- the four stages ------------------------------------------------------

$rows = @()
$rows += Measure-Stage "S0" "no WinRemap window (control)"

Check "log-opens" (Open-Log) "tray menu -> invoke 1004 (Show log)"
$rows += Measure-Stage "S1" "the log window is open"

Close-WindowLike "*log*" | Out-Null
Start-Sleep -Seconds 2
Say ("log window still listed: " + [bool](Get-WindowLike "*log*"))
$rows += Measure-Stage "S2" "the log window was opened and closed"

Check "settings-opens" (Open-Settings) "tray menu -> invoke 1003 (Settings)"
$rows += Measure-Stage "S3" "the settings window is open"

# --- what the three records say -------------------------------------------

Say ""
Say "=== summary ==="
# `{2,8}` right-aligns in .NET; `{2,>8}` is not a format specifier and throws
# at the point of use - which killed the first run of this probe after every
# measurement had been taken and before any of them was printed.
Say ("{0,-4} {1,-32} {2,8} {3,8}  {4}" -f "", "state", "listener", "winremap", "remapped")
foreach ($r in $rows) {
    Say ("{0,-4} {1,-32} {2,8} {3,8}  {4}" -f $r.Stage, $r.State, $r.Listener, $r.Reported, $r.Remapped)
}

# The two questions the table answers, as assertions so a re-run says whether
# the picture changed. They are written the way the *fixed* app would behave,
# so today they are expected to fail - a probe whose checks all pass on a
# broken app would be recording the breakage as correct.
$s1 = @($rows | Where-Object { $_.Stage -eq "S1" })[0]
$s3 = @($rows | Where-Object { $_.Stage -eq "S3" })[0]
Check "the-event-reaches-another-client" ($s1.Listener -ge 1) `
    "with the log window open, an independent hook in another process is told about the switch"
Check "winremap-acts-on-it-with-a-window-open" ($s1.Reported -ge 1) `
    "with the log window open, WinRemap reports the same switch"
Check "the-keymap-applies-with-a-window-open" ([bool]$s1.Remapped) `
    "with the log window open, the rule scoped to Notepad still applies"
Check "the-settings-window-behaves-the-same-way" ([bool]$s3.Remapped) `
    "the settings window is not special: whatever holds for the log window holds here"

# --- the raw records ------------------------------------------------------

Say ""
Say "=== listener (independent EVENT_SYSTEM_FOREGROUND client) ==="
foreach ($l in (Read-Utf8 $listenerOut)) { Say ("  L| " + $l) }

Say ""
Say "=== winremap --debug transcript (window lines only) ==="
foreach ($l in @((Read-Utf8 $debugOut) | Where-Object { $_ -match "\[window\]|\[decided\]|\[action\]" })) { Say ("  D| " + $l) }

Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

Say ""
Say ("=== wall clock: {0:mm\:ss} ===" -f $total.Elapsed)
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
