# Runs inside the guest, in session 1. Check 03-tray-actions with the agent
# taken out of the loop: enable/disable, reload, quit.
#
# Every verdict is read off the tray icon's own name, never off the menu. The
# menu is a Win32 popup (class #32768) whose items are exposed to UI Automation
# only sometimes; the icon is always in the tree, and its name is its tooltip:
#
#   remapping on   -> "WinRemap - 1 keymap(s)"
#   remapping off  -> "WinRemap (disabled)"
#   reload failed  -> the name says FAILED
#
# So the menu is used to *act* and the icon to *judge*, which is the rule
# docs/05_ui-test-automation.md states for the tray. The acting is winapp's
# `invoke <command id>` - by id, so the check does not depend on the guest's
# display language the way a first-letter keystroke does.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled.

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\03-tray-actions.txt"

$lines = New-Object System.Collections.Generic.List[string]
$checks = [ordered]@{}
$total = [System.Diagnostics.Stopwatch]::StartNew()
function Say([string]$s) { $lines.Add($s) }
function Check([string]$name, $Ok, [string]$detail) {
    $ok = if ($Ok -is [bool]) { $Ok } else { @($Ok | Where-Object { $_ }).Count -gt 0 }
    $checks[$name] = $ok
    Say ("CHECK {0,-28} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}
function Flush { $lines | Set-Content $outPath -Encoding UTF8 }
trap { Say "EXCEPTION: $_"; Say $_.ScriptStackTrace; Flush; exit 1 }

. "$PSScriptRoot\ui-helpers.ps1"
. "$PSScriptRoot\winapp-helpers.ps1"

Say ("winapp " + (W @("--version")).Text)
Say ("started " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))

# The taskbar, where the promoted icon lives. Read through winapp like
# everything else; only the right-click that opens the menu is plain Win32,
# because winapp refuses to click a window that is not in the foreground and
# the taskbar never is.
$trayWindow = $null
foreach ($w in (WinappWindows "explorer")) {
    if ($w.className -eq "Shell_TrayWnd" -and -not $trayWindow) { $trayWindow = $w }
}
Check "taskbar-found" ([bool]$trayWindow) $(if ($trayWindow) { "Shell_TrayWnd hwnd=" + $trayWindow.hwnd } else { "no Shell_TrayWnd" })
if (-not $trayWindow) { Flush; exit 1 }
$trayTarget = @("-w", "$($trayWindow.hwnd)")

# The notification-area icon, matched by name.
#
# Not by selector: a promoted icon does not carry the `btn-systemtrayicon-*`
# selector the ones in the overflow do, and filtering on it found nothing while
# the icon sat in the tree the whole time (measured 2026-07-31 - the first run
# of this check reported "no icon" five times over with WinRemap running).
#
# The taskbar button of an open window is also named after WinRemap, but this
# check never opens a window, so a name match cannot pick up the wrong one.
# The whole taskbar is dumped once so a future mismatch is readable rather than
# a silent empty string.
$script:dumpedTaskbar = $false
function Get-IconName {
    $els = WinappElements $trayTarget
    if (-not $script:dumpedTaskbar) {
        Say ("taskbar elements: " + $els.Count)
        foreach ($e in $els) { Say ("  " + (WinappType $e) + " '" + (WinappName $e) + "' selector=" + $e.selector) }
        $script:dumpedTaskbar = $true
    }
    # The shortest match, not the first. `inspect` returns containers as well
    # as leaves, and a container's name is the run-together names of everything
    # under it: with the icon disabled, the first match read
    # "WinRemap - 1 keymap(s) WinRemap (disabled)" - both states at once. An
    # assertion on that says whatever the reader hoped it would.
    $best = ""
    foreach ($e in $els) {
        $n = WinappName $e
        if ($n -notlike "*WinRemap*") { continue }
        if (-not $best -or $n.Length -lt $best.Length) { $best = $n }
    }
    return $best
}

Say ""
Say "=== launch ==="
Start-App "C:\Test\minimal.toml" | Out-Null
Start-Sleep -Seconds 3
$name0 = Get-IconName
Say ("icon: '" + $name0 + "'")
Check "icon-present" ([bool]$name0) "the notification-area icon is in the tree"
Check "starts-enabled" ($name0 -like "*keymap(s)*") "the tooltip names the keymap count"

# Each step: act through the menu, then read the icon. Sleeps are short and
# fixed here on purpose - the tooltip is not an element that appears or
# disappears, so there is no transition for wait-for to watch.
Say ""
Say "=== Enabled (off) ==="
Check "toggle-off-acted" (Toggle-Enabled) "tray menu -> invoke 1001"
Start-Sleep -Seconds 2
$name1 = Get-IconName
Say ("icon: '" + $name1 + "'")
# "disabled" appears, and - the half that carries the weight - it is gone again
# after step 4. Requiring the keymap count to disappear here would be wrong:
# measured, the disabled name reads "WinRemap - 1 keymap(s) WinRemap
# (disabled)". The shell's accessible name for the notification area runs the
# old tooltip and the new one together, and no shorter element separates them.
# So the discriminator is the word "disabled" alone, asserted in both
# directions across the two steps.
Check "reports-disabled" ($name1 -like "*disabled*") "'$name1'"

Say ""
Say "=== Enabled (on again) ==="
Check "toggle-on-acted" (Toggle-Enabled) "tray menu -> invoke 1001"
Start-Sleep -Seconds 2
$name2 = Get-IconName
Say ("icon: '" + $name2 + "'")
Check "reports-enabled" (($name2 -like "*keymap(s)*") -and ($name2 -notlike "*disabled*")) "'$name2'"

Say ""
Say "=== Reload config ==="
Check "reload-acted" (Reload-Config) "tray menu -> invoke 1002"
Start-Sleep -Seconds 2
$name3 = Get-IconName
Say ("icon: '" + $name3 + "'")
Check "reload-succeeded" (($name3 -notlike "*FAILED*") -and ($name3 -like "*keymap(s)*") -and ($name3 -notlike "*disabled*")) "'$name3'"

Say ""
Say "=== Quit ==="
Check "quit-acted" (Quit-App) "tray menu -> invoke 1005"
# Quitting is the one transition with something to wait for, and Get-Process is
# what waits for it: winapp's --gone cannot judge a process that is going away,
# because losing the process makes resolving --app fail in the first place.
$gone = $false
foreach ($i in 1..20) {
    if (-not (Get-Process winremap -ErrorAction SilentlyContinue)) { $gone = $true; break }
    Start-Sleep -Milliseconds 500
}
Check "quit-exits" $gone "no winremap process is left"

Say ""
Say ("=== wall clock: {0:mm\:ss} ===" -f $total.Elapsed)
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
