# Runs inside the guest, in session 1. Scenario 01-settings-window with the
# agent taken out of the loop: the settings window opens from the tray menu and
# every element it must expose is asserted through the Windows App Development
# CLI (`winapp`).
#
# This is Phase 2 of docs/v0.5/notes/20260727_winapp-cli-migration.md - one
# scenario ported, to answer whether the flakiness goes away. The question the
# migration exists for is not "can winapp do it" but "does it decide the same
# thing every time", so this script is run five times in a row and the five
# verdicts are compared (v0.7 plan section 3.3).
#
# What the agent version asserted, and this one keeps:
#   - a Button named "Edit"
#   - text elements named "General" and "Keymaps"
#   - a text element named "notepad" (the keymap in minimal.toml)
#   - a text element whose name starts with "WinRemap v"
# Pressing Edit is deliberately NOT part of this check; 00-uia-actuation covers
# actuation, and keeping the split means a failure here is about what the
# window exposes rather than about what a button does.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled.

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\winapp-settings-window.txt"

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

# Start-App and Open-TrayMenu come from here; the winapp wrappers from the
# other. Opening the menu stays a real right-click on the promoted icon: winapp
# refuses to click a window that is not in the foreground, and the taskbar never
# is (P6(a) of the gate). Choosing the item is winapp's job - by command id, so
# the check does not depend on the guest's display language.
. "$PSScriptRoot\ui-helpers.ps1"
. "$PSScriptRoot\winapp-helpers.ps1"

Say ("powershell " + $PSVersionTable.PSVersion)
Say ("winapp " + (W @("--version")).Text)
Say ("started " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))

# =========================================================================
# open it
# =========================================================================
Say ""
Say "=== opening the settings window ==="
Start-App "C:\Test\minimal.toml" | Out-Null
$running = [bool](Get-Process winremap -ErrorAction SilentlyContinue)
Check "app-running" $running "winremap.exe is resident after launch"
if (-not $running) { Flush; exit 1 }

$opened = Open-Settings
Check "settings-opens" $opened "tray menu -> invoke 1003 (Settings) returned success"

# winapp's own view of the process's windows, not the UIA client's: if the two
# disagreed, everything below would be measuring the wrong tool.
$windows = WinappWindows "winremap"
Say ("windows winapp can see: " + $windows.Count)
foreach ($w in $windows) { Say ("  '" + $w.title + "'  class=" + $w.className + " hwnd=" + $w.hwnd) }
$settings = WinappWindow "winremap" "*ettings*"
Check "settings-window-listed" ([bool]$settings) `
    $(if ($settings) { "title '" + $settings.title + "'" } else { "no window whose title contains 'ettings'" })
if (-not $settings) { Flush; exit 1 }
$target = @("-w", "$($settings.hwnd)")

# =========================================================================
# what it exposes
# =========================================================================
Say ""
Say "=== elements ==="
$els = WinappElements $target
Say ("elements: " + $els.Count)
foreach ($e in $els) { Say ("  " + (WinappType $e) + " '" + (WinappName $e) + "' selector=" + $e.selector) }

# The floor, not the exact number: the count moves with the config and with
# egui's own widgets, and pinning it would turn every unrelated GUI change into
# a red run. Plain UIA saw 43 here; winapp counts a few differently.
Check "element-count" ($els.Count -ge 40) "$($els.Count) elements (plain UIA sees 43)"

$edit = WinappFind $els "Edit" "Button"
Check "edit-button" ($edit.Count -eq 1) "$($edit.Count) Button named 'Edit'"

foreach ($name in @("General", "Keymaps")) {
    $hit = WinappFind $els $name
    Check ("text-" + $name.ToLower()) ($hit.Count -ge 1) "$($hit.Count) element(s) named '$name'"
}

# The keymap from minimal.toml. Its presence is what says the window is showing
# the config that was loaded and not an empty shell.
$keymap = WinappFind $els "notepad"
Check "keymap-notepad" ($keymap.Count -ge 1) "$($keymap.Count) element(s) named 'notepad'"

$version = WinappFindLike $els "WinRemap v*"
Check "version-line" ($version.Count -ge 1) `
    $(if ($version.Count) { "'" + (WinappName $version[0]) + "'" } else { "no element named 'WinRemap v...'" })

# =========================================================================
# the assertions can fail
# =========================================================================
# A check that only ever looks for things that are there proves nothing about
# the tool: it would pass just as happily against a window it never found. So
# ask for something that cannot exist, and require the answer to be no.
Say ""
Say "=== negative cases ==="
$absent = WinappFind $els "NoSuchElementInThisWindow"
Check "absent-element" ($absent.Count -eq 0) "a name nothing carries matches $($absent.Count) elements"
$waitAbsent = WinappWaitFor "NoSuchElementInThisWindow" $target 2000
Check "absent-wait-for" (-not $waitAbsent) "wait-for on a missing element exits non-zero"

# And the positive form of the same question, through the code path the ported
# scenarios will actually use for transitions.
$waitEdit = WinappWaitFor "Edit" $target 5000
Check "wait-for-edit" $waitEdit "wait-for finds the Edit button with no fixed sleep"

# =========================================================================
Say ""
Say ("=== wall clock: {0:mm\:ss} ===" -f $total.Elapsed)
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
