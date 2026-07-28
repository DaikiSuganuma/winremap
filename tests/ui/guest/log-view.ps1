# Runs inside the guest, in session 1. Checks the log window's two views
# (ADR 0057): one line per key by default, the whole stream when "Every event"
# is ticked.
#
# It also leaves two screenshots behind - one of each view - because half of
# what this feature is about is whether the result is readable, and no
# assertion covers that.
#
# Needs the test-inject build and --accept-injected (ADR 0053): the keys are
# sent with keybd_event, which is an injection, and a shipped build passes
# injections through untouched. Without the flag there would be no decision to
# log and the checks would be measuring nothing.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so the arrows the log draws are built from their code points below.
#
# Host side: .\run-vm-ui-test.ps1 -Scenario 00-log-view

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\log-view.txt"

$lines = New-Object System.Collections.Generic.List[string]
$checks = [ordered]@{}
function Say([string]$s) { $lines.Add($s) }
# $Ok is deliberately untyped, as in the other guest scripts: `-match` against
# an array returns the matching elements rather than a boolean, and a [bool]
# parameter then throws mid-run, losing every check after it.
function Check([string]$name, $Ok, [string]$detail) {
    $ok = if ($Ok -is [bool]) { $Ok } else { @($Ok | Where-Object { $_ }).Count -gt 0 }
    $checks[$name] = $ok
    Say ("CHECK {0,-34} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}
function Flush { $lines | Set-Content $outPath -Encoding UTF8 }
trap { Say "EXCEPTION: $_"; Say $_.ScriptStackTrace; Flush; exit 1 }

. "$PSScriptRoot\ui-helpers.ps1"
Add-Type -AssemblyName System.Drawing

# The log draws these; a literal here would be mangled by CP932.
$DOWN = [string][char]0x2193
$UP = [string][char]0x2191

Say ("powershell " + $PSVersionTable.PSVersion)
Say ("started " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))

function Get-Toggle($e) {
    try {
        return "$($e.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState)"
    }
    catch { return "unknown" }
}

function Set-Toggle($e) {
    try {
        $e.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
        Start-Sleep -Seconds 2
        return $true
    }
    catch { Click-Element $e; return $true }
}

function Save-Shot([string]$path, $w) {
    if (-not $w) { return }
    $r = $w.Current.BoundingRectangle
    $bmp = New-Object System.Drawing.Bitmap([int]$r.Width, [int]$r.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose()
    $bmp.Dispose()
    Say ("  screenshot: $path  " + [int]$r.Width + "x" + [int]$r.Height)
}

function Say-Lines([string]$label, $texts) {
    Say ("--- $label (" + @($texts).Count + " text elements) ---")
    foreach ($t in $texts) { Say ("  | " + $t) }
}

# --- launch and open the log ----------------------------------------------
Start-App "C:\Test\logview.toml" "en" @('--accept-injected') | Out-Null
Say ("winremap running: " + [bool](Get-Process winremap -ErrorAction SilentlyContinue))
Open-Log | Out-Null
$log = Get-WindowLike "*log*"
Say ("log window: " + $(if ($log) { "'" + $log.Current.Name + "'" } else { "NOT OPEN" }))

$box = Find-Named $log "Every event"
Check "the-checkbox-is-there" ([bool]$box) `
    "the log header offers the detailed view beside Follow newest"
Check "the-simple-view-is-the-default" ((Get-Toggle $box) -eq "Off") `
    "one line per key is what the window opens with (ADR 0057)"

# --- type, with Notepad in front ------------------------------------------
# Not into the log window itself, which is the obvious way and does not work:
# keys injected while WinRemap's own window is in the foreground do not reach
# its hook, while the same keys sent to Notepad do (measured 2026-07-28 - see
# docs/05_ui-test-automation.md). Typing into another application is also what
# the rest of the suite does, and what a user does.
#
# `a` is not in the config and passes through; C-n is, and its target needs no
# modifiers, so the held Ctrl has to be lifted before it and put back after -
# and the putting-back happens on release, which is the whole point of the
# time column.
Start-Process notepad.exe
Start-Sleep -Seconds 5
$pad = $null
foreach ($w in $root.FindAll($kids, $any)) { if ($w.Current.ClassName -eq "Notepad") { $pad = $w; break } }
# Asserted, not assumed: without this the keys would go to whatever is in
# front, the log would stay empty, and every check below would blame the app.
Check "notepad-takes-the-keys" ([bool]$pad -and (Focus-Window $pad)) `
    "the keys go to another application, which is the only way they reach the hook"
[Nat]::Key(0x41)
Start-Sleep -Milliseconds 500
[Nat]::Chord(0x11, 0x4E)
Start-Sleep -Seconds 2
Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
Focus-Window (Get-WindowLike "*log*") | Out-Null

$simple = Get-Texts (Get-WindowLike "*log*")
Say-Lines "simple view" $simple
Save-Shot "C:\Test\log-view-simple.png" (Get-WindowLike "*log*")

Check "the-decision-is-shown" ($simple -match "C-n" -and $simple -match "Down") `
    "the remap WinRemap chose is on screen"
Check "the-pass-through-is-shown" ($simple -match "passed through") `
    "a key with no rule says so, rather than saying nothing"
Check "the-simple-view-hides-the-mechanics" (-not (@($simple | Where-Object { $_ -match "\[injected\]|\[input\]" }).Count)) `
    "the injections and the physical events stay out of the default view"
Check "every-line-carries-a-time" ($simple -match "^\d\d:\d\d:\d\d\.\d\d\d$") `
    "the stamp is its own column, to the millisecond, which is what tells two moments apart"

# --- tick the box, without pressing another key ---------------------------
# The point of buffering the mechanics whether or not they are shown: someone
# ticks this after something has already gone wrong.
Set-Toggle $box | Out-Null
$detailed = Get-Texts (Get-WindowLike "*log*")
Say-Lines "detailed view" $detailed
Save-Shot "C:\Test\log-view-detailed.png" (Get-WindowLike "*log*")

Check "the-box-turns-on" ((Get-Toggle $box) -eq "On") "the checkbox took the click"
Check "it-explains-what-already-happened" ($detailed -match "\[injected\]") `
    "ticking the box shows the mechanics behind keys pressed before it was ticked"
Check "the-physical-events-are-shown" ($detailed -match "\[input\]") `
    "the detailed view is a transcript: presses and releases, not just decisions"
Check "the-modifier-surgery-is-visible" (@($detailed | Where-Object { $_ -like "LCtrl*" }).Count -ge 2) `
    "Ctrl is lifted before the target and put back after (ADR 0005), and both show"
Check "both-halves-of-the-remap-are-visible" `
(@($detailed | Where-Object { $_ -like "Down*$DOWN*" }).Count -ge 1 -and
    @($detailed | Where-Object { $_ -like "Down*$UP*" }).Count -ge 1) `
    "the target's press and its release are both on screen"

# --- the clipboard has to carry what the columns say ----------------------
Set-Clipboard -Value "cleared-before-the-check"
$copy = Find-Named (Get-WindowLike "*log*") "Copy all"
if ($copy) { Invoke-Element $copy | Out-Null } else { Say "  Copy all not found" }
Start-Sleep -Seconds 1
[string]$clip = (Get-Clipboard) -join "`n"
Say ("clipboard: " + $clip.Length + " chars")
Check "the-clipboard-keeps-the-columns" ($clip -match "(?m)^\d\d:\d\d:\d\d\.\d\d\d \[decided\] ") `
    "a pasted log still says when each line happened and what kind it is"

# --- and back ------------------------------------------------------------
Set-Toggle $box | Out-Null
$back = Get-Texts (Get-WindowLike "*log*")
Check "unticking-restores-the-simple-view" (-not (@($back | Where-Object { $_ -match "\[injected\]" }).Count)) `
    "the view is a filter, not a switch that has to be set before the fact"

# The same keys again, with no WinRemap window open and the transcript going
# to a file. It is the same feature seen from the other side - a terminal
# session - and it is what tells "the hook never saw the key" apart from "the
# window never showed what the hook saw", which look identical from outside.
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
$dbg = "C:\Test\debug-out.txt"
Remove-Item $dbg -Force -ErrorAction SilentlyContinue
Start-Process $exe -ArgumentList '--config', 'C:\Test\logview.toml', '--lang', 'en', '--accept-injected', '--debug' `
    -NoNewWindow -RedirectStandardOutput $dbg
Start-Sleep -Seconds 6
[Nat]::Key(0x41)
Start-Sleep -Milliseconds 500
[Nat]::Chord(0x11, 0x4E)
Start-Sleep -Seconds 3
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
# Read as UTF-8: the arrows are outside CP932, which is what Get-Content
# assumes here, and a mangled transcript would fail the check for the wrong
# reason.
[string[]]$console = if (Test-Path $dbg) { Get-Content $dbg -Encoding UTF8 } else { @() }
Say "console transcript:"
foreach ($l in $console) { Say ("  C| " + $l) }
Check "the-console-says-the-same" `
(@($console | Where-Object { $_ -like "``[decided``]*C-n*" }).Count -ge 1 -and
    @($console | Where-Object { $_ -like "``[injected``]*" }).Count -ge 4) `
    "a terminal session gets the same tags, without the window (ADR 0016)"

$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
