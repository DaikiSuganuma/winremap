# Runs inside the guest, in session 1. Check 05-remap-notepad with the agent
# taken out of the loop: does a rule in a keymap actually change what a real
# application receives?
#
# This is the one scenario where the migration cannot remove the keystroke
# itself. WinRemap's hook only sees a key that Windows delivers as input, and
# winapp's default `--via post-message` posts a window message instead - it
# never passes a low-level hook, so it would test nothing at all. `--via
# send-input` is SendInput, which is an injection, which is why this check
# needs the test-inject build and --accept-injected (ADR 0053) exactly as the
# agent scenario did.
#
# Which mechanism actually delivers the chord is measured here rather than
# assumed: each attempt is followed by a read of Notepad's text, and the
# mechanism that moved it is the one the verdict is based on. keybd_event is
# the fallback, and it is not a lesser one - it is what tests/ui/guest/
# log-view.ps1 has been pressing keys with all along.
#
# The negative control at the end is the point of the whole check. "The
# find/replace panel did not open" is only worth reading if this script can
# see the panel when it does open, so once the verdict is in, WinRemap is
# stopped and the same chord is sent again: Notepad's own Ctrl+H must open the
# panel and this script must notice.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled - and this guest's Windows is
# Japanese, so the panel's own labels are built from code points below.
#
# Host side: .\run-vm-ui-test.ps1 -Check 05-remap-notepad

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\05-remap-notepad.txt"

$lines = New-Object System.Collections.Generic.List[string]
$checks = [ordered]@{}
$total = [System.Diagnostics.Stopwatch]::StartNew()
function Say([string]$s) { $lines.Add($s) }
# $Ok is deliberately untyped, as in the other guest scripts: `-match` against
# an array returns the matching elements rather than a boolean, and a [bool]
# parameter then throws mid-run, losing every check after it.
function Check([string]$name, $Ok, [string]$detail) {
    $ok = if ($Ok -is [bool]) { $Ok } else { @($Ok | Where-Object { $_ }).Count -gt 0 }
    $checks[$name] = $ok
    Say ("CHECK {0,-30} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}
function Flush { $lines | Set-Content $outPath -Encoding UTF8 }
trap { Say "EXCEPTION: $_"; Say $_.ScriptStackTrace; Flush; exit 1 }

. "$PSScriptRoot\ui-helpers.ps1"
. "$PSScriptRoot\winapp-helpers.ps1"

# The find/replace panel's labels, in both display languages. A literal here
# would be mangled by CP932, so the two Japanese words - "search" (U+691C
# U+7D22) and "replace" (U+7F6E U+63DB) - are built from their code points.
$FIND_JA = [string][char]0x691C + [string][char]0x7D22
$REPLACE_JA = [string][char]0x7F6E + [string][char]0x63DB
$PANEL_WORDS = @("Replace", "Find", $REPLACE_JA, $FIND_JA)

Say ("winapp " + (W @("--version")).Text)
Say ("powershell " + $PSVersionTable.PSVersion)
Say ("started " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))

# Recorded, not relied on: `send-keys` is where this port could break when the
# CLI moves (the tool is public preview), and its own help is the only account
# of what it accepts that is guaranteed to match the build in this guest.
Say ""
Say "=== winapp ui send-keys --help ==="
foreach ($l in ((W @("ui", "send-keys", "--help")).Text -split "\r?\n")) { Say ("  | " + $l) }

# --- Notepad, as winapp sees it -------------------------------------------

$script:padTarget = @()
$script:dumpedPad = $false

function Get-PadWindow {
    foreach ($w in (WinappWindows "notepad")) { return $w }
    return $null
}

# Notepad's text. The document is an Edit/Document element whose *name* is
# empty - what it holds is a value, so `inspect` alone cannot see it and
# get-property is the only way through winapp (the same trap the settings
# window's combo box sprang, see winapp-helpers.ps1).
function Get-PadTextWinapp {
    $els = WinappElements $script:padTarget
    if (-not $script:dumpedPad) {
        Say ("notepad elements: " + $els.Count)
        foreach ($e in $els) { Say ("  " + (WinappType $e) + " '" + (WinappName $e) + "' selector=" + $e.selector) }
        $script:dumpedPad = $true
    }
    foreach ($e in $els) {
        if ((WinappType $e) -notmatch "Document|Edit") { continue }
        if ($e.PSObject.Properties.Name -notcontains "selector" -or -not $e.selector) { continue }
        $v = WinappProperty $e.selector "Value" $script:padTarget
        if ($v) { return $v }
    }
    return ""
}

# The same question asked through a plain UIA client. Not a duplicate: when the
# two disagree, the disagreement is the finding - it is the difference between
# "winapp reads it wrong" and "the app did not do it", which is the distinction
# this whole migration exists to keep.
function Get-PadTextUia {
    $pad = $null
    foreach ($w in $root.FindAll($kids, $any)) { if ($w.Current.ClassName -eq "Notepad") { $pad = $w; break } }
    if (-not $pad) { return "" }
    foreach ($e in $pad.FindAll($desc, $any)) {
        $t = "$($e.Current.ControlType.ProgrammaticName)"
        if ($t -notmatch "Document|Edit") { continue }
        try {
            $v = $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
            if ($v) { return [string]$v }
        }
        catch { }
    }
    return ""
}

# What the checks judge. Trailing newlines are Notepad's, not the test's.
function Get-PadText {
    $v = Get-PadTextWinapp
    if (-not $v) { $v = Get-PadTextUia }
    return ([string]$v).Trim()
}

# Reads until the document says what it is expected to, or gives up and returns
# whatever it last saw. A fixed sleep is not enough: the third run of this check
# read the document 800 ms after typing "abc" and found "ab" - the window was
# still titled "*ab - Notepad" - and so recorded keybd_event as the mechanism
# that had worked when winapp's had, in fact, worked. Nothing failed, which is
# the point: without this the *record* of how the keys got there is a coin toss.
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

function Get-PadNames {
    $names = @()
    foreach ($e in (WinappElements $script:padTarget)) {
        $n = WinappName $e
        if ($n) { $names += $n }
    }
    return , @($names | Sort-Object -Unique)
}

# Names that are in the tree now and were not before. The panel is drawn inside
# Notepad's own window, so a window list cannot see it (the agent scenario said
# as much); a name that appeared is what it leaves behind. Comparing against a
# baseline rather than matching a fixed list is what keeps a menu item that was
# always there from reading as an open panel.
function Get-PanelHits($before, $after) {
    $new = @($after | Where-Object { $before -notcontains $_ })
    $hits = @()
    foreach ($n in $new) {
        foreach ($w in $PANEL_WORDS) { if ($n -like "*$w*") { $hits += $n; break } }
    }
    return , @($hits)
}

# --- sending the keys ------------------------------------------------------
#
# Each attempt is judged by what Notepad holds afterwards, never by the exit
# code alone: a spelling winapp accepts and delivers as literal text exits 0
# and types garbage, which would otherwise be recorded as a working mechanism.
$script:typeVia = "?"
$script:chordVia = "?"
# The spelling that turned out to press the chord rather than type it. Kept
# because the negative control has to send the *same* keys again: the first
# run of this check sent "^h" there while "ctrl+h" was what had worked, typed
# two literal characters into Notepad, and reported that the panel never
# opened - a failure that was entirely this script's (measured 2026-07-31).
$script:chordKeys = ""

function Try-WinappKeys([string]$Keys) {
    $r = W (@("ui", "send-keys", $Keys, "--via", "send-input") + $script:padTarget)
    if ($r.Code -ne 0) {
        Say ("  send-keys '" + $Keys + "' exit " + $r.Code + ": " + ($r.Text -replace "\r?\n", " / "))
        return $false
    }
    Start-Sleep -Milliseconds 800
    return $true
}

function Clear-Pad {
    # Plain keybd_event on purpose: this is housekeeping between measurements,
    # and it must not depend on the mechanism being measured.
    [Nat]::Chord(0x11, 0x41)   # Ctrl+A
    Start-Sleep -Milliseconds 300
    [Nat]::Key(0x2E)           # Delete
    Start-Sleep -Milliseconds 500
}

function Type-Abc {
    foreach ($vk in @(0x41, 0x42, 0x43)) { [Nat]::Key([byte]$vk) }
    Start-Sleep -Milliseconds 500
}

# --- run -------------------------------------------------------------------

Say ""
Say "=== launch ==="
Start-App "C:\Test\uitest.toml" "en" @('--accept-injected') | Out-Null
Check "winremap-running" ([bool](Get-Process winremap -ErrorAction SilentlyContinue)) `
    "the test-inject build is up with --accept-injected (ADR 0053)"

Start-Process notepad.exe
Start-Sleep -Seconds 5
$padWindow = Get-PadWindow
Check "notepad-found" ([bool]$padWindow) `
    $(if ($padWindow) { "hwnd=" + $padWindow.hwnd + " '" + $padWindow.title + "'" } else { "winapp lists no notepad window" })
if (-not $padWindow) { Flush; exit 1 }
$script:padTarget = @("-w", "$($padWindow.hwnd)")

# Focus is plain Win32, as in log-view.ps1: keys go to the foreground window,
# and a check that types into the wrong one reports the application as broken.
$padUia = $null
foreach ($w in $root.FindAll($kids, $any)) { if ($w.Current.ClassName -eq "Notepad") { $padUia = $w; break } }
Check "notepad-takes-the-keys" ([bool]$padUia -and (Focus-Window $padUia)) `
    "the keys go to another application, which is the only way they reach the hook"
Start-Sleep -Seconds 1

Say ""
Say "=== type abc ==="
# The typing is measured the same way as the chord, because if winapp's
# send-keys cannot even put three letters in, the chord result would be about
# the tool rather than about the remap.
if ((Try-WinappKeys "abc") -and (Wait-PadText '^abc$') -eq "abc") {
    $script:typeVia = "winapp send-keys --via send-input"
}
else {
    Clear-Pad
    Type-Abc
    $script:typeVia = "keybd_event"
}
$typed = Get-PadText
Say ("typed via: " + $script:typeVia)
Say ("notepad text: '" + $typed + "'  (uia says '" + (Get-PadTextUia).Trim() + "')")
Check "typing-lands-in-notepad" ($typed -eq "abc") `
    "the editing area holds exactly abc before the remap is tested"
if ($typed -ne "abc") { Say "cannot judge a remap on top of the wrong text"; Flush; exit 1 }

$namesBefore = Get-PadNames
Say ("named elements before the chord: " + $namesBefore.Count)

Say ""
Say "=== Ctrl+H ==="
# Spellings tried in order. None of them is documented for this build, which is
# why the effect decides rather than the exit code, and why the help output is
# in this file above.
foreach ($spelling in @("ctrl+h", "^h", "{Ctrl}h")) {
    if (-not (Try-WinappKeys $spelling)) { continue }
    $now = Wait-PadText '^(xabc|abcx)$'
    Say ("  after '" + $spelling + "': '" + $now + "'")
    if ($now -eq "abc") { continue }             # accepted, delivered nothing
    if ($now -match '^(xabc|abcx)$') {
        $script:chordVia = "winapp send-keys '" + $spelling + "' --via send-input"
        $script:chordKeys = $spelling
        break
    }
    # Delivered something else - literal text, most likely. Put the document
    # back the way it was and keep looking.
    Say ("  '" + $spelling + "' did not arrive as a chord; restoring abc")
    Clear-Pad
    Type-Abc
}
if ($script:chordVia -eq "?") {
    [Nat]::Chord(0x11, 0x48)
    Start-Sleep -Seconds 2
    $script:chordVia = "keybd_event"
}
Say ("chord via: " + $script:chordVia)

$after = Get-PadText
$namesAfter = Get-PadNames
$panelHits = Get-PanelHits $namesBefore $namesAfter
Say ("notepad text: '" + $after + "'  (uia says '" + (Get-PadTextUia).Trim() + "')")
Say ("named elements after the chord: " + $namesAfter.Count)
foreach ($n in @($namesAfter | Where-Object { $namesBefore -notcontains $_ })) { Say ("  new: '" + $n + "'") }

# Where the x lands does not matter - a key sent to a window can move the caret
# to the start of the document - so both ends are correct. What matters is that
# exactly one arrived and nothing else changed.
Check "the-remap-types-x" ($after -match '^(xabc|abcx)$') `
    "Ctrl+H put one x into the document and left abc alone ('$after')"
Check "the-panel-stays-closed" ($panelHits.Count -eq 0) `
    $(if ($panelHits.Count) { "the panel opened: " + ($panelHits -join ', ') } else { "no find/replace element appeared; WinRemap swallowed Notepad's own Ctrl+H" })

# --- the negative control --------------------------------------------------
# Without this the check above is unfalsifiable: a script that can never see
# the panel passes "the panel stays closed" on a machine where the remap does
# nothing at all. So take WinRemap away and press the same chord again.
Say ""
Say "=== negative control: the same chord with no WinRemap ==="
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
Check "winremap-is-gone" (-not (Get-Process winremap -ErrorAction SilentlyContinue)) `
    "nothing is left to remap the chord"
Check "notepad-still-takes-the-keys" (Focus-Window $padUia) `
    "the control chord goes to the same window the measured one did"
Start-Sleep -Seconds 1
$controlBefore = Get-PadNames
$textBefore = Get-PadText
# The same keys, sent the same way. Anything else measures a different thing
# and cannot support the assertion it is there to prove.
if ($script:chordKeys) { Try-WinappKeys $script:chordKeys | Out-Null } else { [Nat]::Chord(0x11, 0x48) }
Start-Sleep -Seconds 3
$controlAfter = Get-PadNames
$controlHits = Get-PanelHits $controlBefore $controlAfter
$textAfter = Get-PadText
Say ("named elements: " + $controlBefore.Count + " -> " + $controlAfter.Count)
Say ("notepad text: '" + $textBefore + "' -> '" + $textAfter + "'")
# Said out loud because it is the one way this control can mislead: if the
# chord arrived as literal text, the panel was never asked to open and the
# check below would fail for a reason that has nothing to do with the panel.
Check "the-control-chord-is-a-chord" ($textAfter -eq $textBefore) `
    "the control keys pressed a chord rather than typing themselves in ('$textAfter')"
foreach ($n in @($controlAfter | Where-Object { $controlBefore -notcontains $_ })) { Say ("  new: '" + $n + "'") }
Check "the-check-can-see-the-panel" ($controlHits.Count -gt 0) `
    $(if ($controlHits.Count) { "Notepad's own Ctrl+H opened it: " + ($controlHits -join ', ') } else { "the panel did not appear even with no remapper - the assertion above proves nothing" })

Say ""
Say ("=== wall clock: {0:mm\:ss} ===" -f $total.Elapsed)
Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
