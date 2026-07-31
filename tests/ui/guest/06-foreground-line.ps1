# Runs inside the guest, in session 1. Check 06-foreground-line: with the log
# window open, switch the foreground app and see whether the window says so.
#
# This check exists to settle one carried-over observation, not to cover a
# feature. From v0.5 (docs/05_ui-test-automation.md, pitfalls):
#
#   "with the log window open, the app switched to never appeared on the
#    foreground line - the reports named winremap.exe and explorer.exe only,
#    and Notepad never once, even though GetForegroundWindow() said Notepad
#    was in front and its keys were reaching the hook"
#
# It was measured twice on this VM and did NOT reproduce on the developer's
# machine (v0.6 plan section 1). Two environments disagreeing is exactly the
# thing a human is bad at settling and a check is good at, so v0.7 turns the
# observation into an assertion (v0.7 plan section 3.5).
#
# The check answers it in two halves, because "the app never reported it" and
# "the app reported it but the window did not show it" look identical from
# outside and have different owners:
#
#   part 1  log window open   - read the reports off the window
#   part 2  no window at all  - read the reports off --debug's own transcript
#
# Both halves make the same switch, the same way, against the same deadline.
# The deadline is what makes this a check rather than a sleep: "not reported"
# and "not reported yet" are different verdicts, and only a poll with a bound
# tells them apart (a fixed Start-Sleep calls the slow case a failure and the
# broken case a pass, depending on the number that was typed).
#
# No keys are pressed here, so this runs against the same shape of binary that
# ships - no test-inject build. --debug is a shipping flag; the foreground
# report is gated on it (src/window.rs, print_debug_info).
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled. The report line itself
# contains an em dash, which is why every pattern below stops at the quotes
# around the exe name.

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\06-foreground-line.txt"

# How long a foreground switch has to be reported in. The developer's machine
# reported it in 0.2 s; this is 50x that, so a red here means "did not happen",
# not "happened slowly".
$DEADLINE_MS = 10000

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
Add-Type -AssemblyName System.Drawing

# The pixels, for the one question the UI Automation tree cannot answer about
# itself: whether the window is *showing* a line that the tree does not carry.
# Those are two different defects with two different owners - the report never
# happened (ours) or the accessibility tree went stale (egui/AccessKit, ADR
# 0055) - and they look identical from a script that only reads the tree.
function Save-Shot([string]$path, $w) {
    if (-not $w) { Say ("  screenshot: no window for " + $path); return }
    $r = $w.Current.BoundingRectangle
    $bmp = New-Object System.Drawing.Bitmap([int]$r.Width, [int]$r.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose()
    $bmp.Dispose()
    Say ("  screenshot: $path  " + [int]$r.Width + "x" + [int]$r.Height)
}

Say ("winapp " + (W @("--version")).Text)
Say ("started " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))

# The report's shape (src/i18n.rs, debug_foreground):
#   application = "notepad.exe" - matching keymaps: notepad
# Only the part before the dash is matched: the dash is an em dash, outside
# CP932, and this file has to stay ASCII.
$REPORT = '^application = "([a-z0-9_.-]+\.exe)"'
# Same reason $Exe is not the parameter name here: the script's $exe is what
# ui-helpers' Start-App launches.
function Report-For([string]$AppName) { return ('^application = "' + [regex]::Escape($AppName) + '"') }

# Every line the log window is showing, as winapp sees them. One Text element
# per row; the stamp and the tag are their own elements, so a row's name is the
# message alone.
function Log-Lines {
    $out = @()
    foreach ($e in (WinappElements $script:target)) {
        if ((WinappType $e) -notmatch "(?i)text") { continue }
        $n = WinappName $e
        if ($n) { $out += $n }
    }
    return , $out
}

# Polls until at least $Minimum lines match, or the deadline passes. Returns
# how long it took as well as what it found, because "reported after 8.9 s"
# and "reported after 0.2 s" are the same verdict and very different news.
#
# `$seen = & $Read`, NOT `@(& $Read)`. The readers end in `return , $out`, the
# idiom that stops PowerShell 5.1 from unwrapping a one-element array. An
# assignment then hands back the inner array as intended - but @() collects
# *pipeline output*, and the pipeline emits the inner array as ONE object, so
# @() wraps it again. Every line of the window then arrived as a single item
# whose -match is true if ANY line matches, which counted 40 rows as 1 and made
# "switching-back-is-reported" unable to pass however the app behaved. Cost one
# run of this check (the giveaway is a diagnostic line 500 characters long: an
# array printed through string concatenation joins on spaces).
function Wait-Report([scriptblock]$Read, [string]$Pattern, [int]$Minimum) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $seen = @()
    do {
        $seen = & $Read
        $hits = @($seen | Where-Object { $_ -match $Pattern })
        if ($hits.Count -ge $Minimum) {
            return [pscustomobject]@{ Count = $hits.Count; Ms = $sw.ElapsedMilliseconds; Lines = $seen; Hits = $hits }
        }
        Start-Sleep -Milliseconds 300
    } while ($sw.ElapsedMilliseconds -lt $DEADLINE_MS)
    $hits = @($seen | Where-Object { $_ -match $Pattern })
    return [pscustomobject]@{ Count = $hits.Count; Ms = $sw.ElapsedMilliseconds; Lines = $seen; Hits = $hits }
}

# $Rows, not $Lines: PowerShell resolves a variable through the *caller's*
# scope chain, so a parameter named $Lines here would shadow the script's
# $lines (a List) for every Say() called from inside this function - and Say's
# .Add() on an array is "the collection is of a fixed size", thrown from a
# function that does not mention arrays at all. Cost one run of this check.
function Say-Reports([string]$Label, $Rows) {
    $all = @($Rows)
    $reports = @($all | Where-Object { $_ -match $REPORT })
    # The row count is printed as well as the report count: a window that is
    # not being read at all and a window that carries no report look the same
    # from the report count alone.
    Say ("  " + $Label + ": " + $reports.Count + " foreground report(s) in " + $all.Count + " row(s)")
    foreach ($r in $reports) { Say ("  F| " + $r) }
    if ($all.Count) { Say ("  last row: " + $all[$all.Count - 1]) }
}

# Notepad's own window, by class rather than by title: the title carries the
# document name and the display language, and this one is untitled.
function Find-Notepad {
    foreach ($w in $root.FindAll($kids, $any)) { if ($w.Current.ClassName -eq "Notepad") { return $w } }
    return $null
}

# What Notepad holds. The document's *name* is empty - the text is a value - so
# the Value pattern is the only way to it. Read through a plain UIA client
# rather than through winapp because that is all this check needs; 05 keeps the
# winapp reader, and the pair is deliberate (05_ui-test-automation.md).
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

# =========================================================================
# part 1: with the log window open
# =========================================================================
Say ""
Say "=== part 1: the log window is open ==="

# uitest.toml scopes its keymap to notepad.exe, which makes the report say
# something falsifiable: the line for Notepad has to resolve to the keymap
# named notepad, and the line for winremap.exe must not. A global keymap would
# match everything and the second half of the line would prove nothing.
Start-App "C:\Test\uitest.toml" "en" @("--debug", "--accept-injected") | Out-Null
Check "app-running" ([bool](Get-Process winremap -ErrorAction SilentlyContinue)) "winremap.exe is resident"
Check "log-opens" (Open-Log) "tray menu -> invoke 1004 (Show log)"

$logw = WinappWindow "winremap" "*log*"
Check "log-window-listed" ([bool]$logw) $(if ($logw) { "'" + $logw.title + "'" } else { "no window whose title contains 'log'" })
if (-not $logw) { Flush; exit 1 }
$script:target = @("-w", "$($logw.hwnd)")

$before = Log-Lines
Say-Reports "before the switch" $before
# The baseline that makes the later hit mean something: Notepad is not running
# yet, so nothing can name it. Without this, a report left over from an
# earlier step would pass the check below while nothing was ever reported.
Check "no-notepad-line-yet" (@($before | Where-Object { $_ -match (Report-For "notepad.exe") }).Count -eq 0) `
    "before Notepad exists, no report names it"

# The switch itself. Launching is not switching: a process with no foreground
# right cannot put its child in front, and the report for Notepad's own launch
# said "winremap.exe" when this was assumed (measured 2026-07-29). So the log
# window is brought forward first and Notepad after it, and both are verified
# against GetForegroundWindow - the switch has to be this script's doing.
Start-Process notepad.exe
Start-Sleep -Seconds 5
$pad = Find-Notepad
Focus-Window (Get-WindowLike "*log*") | Out-Null
Start-Sleep -Seconds 1
$focused = ([bool]$pad -and (Focus-Window $pad))
Check "notepad-takes-the-front" $focused `
    "the foreground app really changed, by the same call the window watcher listens for"

$got = Wait-Report ${function:Log-Lines} (Report-For "notepad.exe") 1
Say ("  waited " + $got.Ms + " ms")
Say-Reports "after the switch" $got.Lines
# Taken while Notepad is still in front, which is the state under test: what
# the window is showing at the moment the tree says nothing arrived.
Save-Shot "C:\Test\foreground-line-switched.png" (Get-WindowLike "*log*")
$script:withWindow = ($got.Count -ge 1)
Check "the-switch-is-reported" $script:withWindow `
    ("the app switched to appears on the foreground line within " + $DEADLINE_MS + " ms (v0.5 carry-over)")
# The other half of the line: which keymaps that value reaches. It is the half
# that answers "why is my rule not firing here", and it is the half a stale
# cached name would get wrong.
Check "the-report-resolves-the-keymap" `
(@($got.Hits | Where-Object { $_ -match "keymaps: notepad" }).Count -ge 1) `
    "the report for notepad.exe names the keymap scoped to it"

# =========================================================================
# part 1b: is the missing line only a missing line?
# =========================================================================
# The same call that writes the foreground report also refreshes the cache the
# keyboard hook reads to decide which keymaps apply (src/window.rs,
# refresh_foreground_cache). So a report that never happens is not necessarily
# cosmetic: if the callback did not run, the hook still believes the previous
# app is in front, and an application-scoped rule silently stops applying.
#
# That is the difference between a note in the log's documentation and a
# release blocker, so it is measured rather than argued. uitest.toml maps C-h
# to x for notepad.exe only: "abc" becomes "abcx" if the keymap was chosen,
# and stays "abc" (with Notepad's own replace panel opening) if it was not.
#
# Keys are pressed with keybd_event, so this needs the test-inject build and
# --accept-injected (ADR 0053) - injections are passed through by a shipping
# build, which would make the rule look inapplicable for the wrong reason.
Say ""
Say "=== part 1b: does the keymap still apply? ==="
foreach ($vk in @(0x41, 0x42, 0x43)) { [Nat]::Key([byte]$vk) }
$typed = Wait-PadText '^abc$'
Say ("  typed: '" + $typed + "'")
Check "notepad-takes-the-keys" ($typed -eq "abc") `
    "the keys reach the document, so what follows is about the rule and not about the typing"
[Nat]::Chord(0x11, 0x48)
$remapped = Wait-PadText '^(abcx|xabc)$'
Say ("  after C-h: '" + $remapped + "'")
Check "the-keymap-still-applies" ($remapped -match '^(abcx|xabc)$') `
    "with a WinRemap window open, a rule scoped to the app in front is still chosen"

# Back the other way. One direction could be luck - a report that happened to
# be queued from an earlier change - so the switch back has to be reported
# too, and by growth rather than by presence: winremap.exe was already on the
# line before Notepad ever started.
$backBefore = @($got.Lines | Where-Object { $_ -match (Report-For "winremap.exe") }).Count
Focus-Window (Get-WindowLike "*log*") | Out-Null
$back = Wait-Report ${function:Log-Lines} (Report-For "winremap.exe") ($backBefore + 1)
Say ("  waited " + $back.Ms + " ms (winremap.exe reports: " + $backBefore + " -> " + $back.Count + ")")
Say-Reports "after switching back" $back.Lines
# The decisive picture. The window is in front again here, so it has certainly
# repainted: pixels that show a line the tree never carried would mean the
# report happened and the accessibility tree is what went stale.
Save-Shot "C:\Test\foreground-line-back.png" (Get-WindowLike "*log*")
Check "switching-back-is-reported" ($back.Count -gt $backBefore) `
    "the line keeps up with the focus, not just with the first change"

Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

# =========================================================================
# part 2: no WinRemap window at all
# =========================================================================
# The control. Same switch, same deadline, but the reports go to --debug's
# transcript instead of to a window - so a red in part 1 and a green here says
# the gap belongs to the window's presence, and two reds say it belongs to the
# reporting itself. 00-log-view carries this comparison as a diagnostic
# ("W|" lines); here it decides.
Say ""
Say "=== part 2: no window is open (control) ==="
$dbg = "C:\Test\foreground-debug.txt"
Remove-Item $dbg -Force -ErrorAction SilentlyContinue
Start-Process $exe -ArgumentList '--config', 'C:\Test\uitest.toml', '--lang', 'en', '--debug' `
    -NoNewWindow -RedirectStandardOutput $dbg
Start-Sleep -Seconds 6

# Read as UTF-8: the report's dash is outside CP932, and Get-Content would
# assume CP932 on this guest. The file is being written by a live process, so
# a failed read is "not yet" rather than an error.
function Console-Lines {
    if (-not (Test-Path $dbg)) { return , @() }
    try { return , @(Get-Content $dbg -Encoding UTF8 -ErrorAction Stop) } catch { return , @() }
}

Start-Process notepad.exe
Start-Sleep -Seconds 5
$pad2 = Find-Notepad
$focused2 = ([bool]$pad2 -and (Focus-Window $pad2))
Check "notepad-takes-the-front-again" $focused2 "the control makes the same switch"

# The console line carries the stamp and the tag ahead of the message, so the
# pattern is not anchored the way the window's rows are.
$ctl = Wait-Report ${function:Console-Lines} 'application = "notepad\.exe"' 1
Say ("  waited " + $ctl.Ms + " ms")
foreach ($l in @($ctl.Lines | Where-Object { $_ -match 'application = ' })) { Say ("  W| " + $l) }
$script:withoutWindow = ($ctl.Count -ge 1)
Check "the-switch-is-reported-without-a-window" $script:withoutWindow `
    "with no window open, the same switch reaches the transcript"

Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

# =========================================================================
# the verdict on the carry-over
# =========================================================================
# Not a check - the checks above already decide pass or fail. This is the one
# line a reader of the result file needs, in the words of the question that
# was asked in v0.5.
Say ""
Say "=== the v0.5 carry-over ==="
$verdict =
if ($script:withWindow -and $script:withoutWindow) {
    "REPORTED IN BOTH - the v0.5 observation does not reproduce on this guest"
}
elseif (-not $script:withWindow -and $script:withoutWindow) {
    "ONLY WITHOUT A WINDOW - reproduces: the log window's presence is what stops the report"
}
elseif ($script:withWindow -and -not $script:withoutWindow) {
    "ONLY WITH THE WINDOW - the opposite of the v0.5 observation; suspect the control, not the app"
}
else {
    "REPORTED IN NEITHER - the foreground report is not working at all here"
}
Say ("VERDICT: " + $verdict)

Say ""
Say ("=== wall clock: {0:mm\:ss} ===" -f $total.Elapsed)
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
