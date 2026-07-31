# Runs inside the guest, in session 1. Scenario 02-config-display with the
# agent taken out of the loop: the settings window has to show the config that
# was actually loaded, and selecting a keymap has to show that keymap's rules.
#
# What the agent version asserted, and this one keeps:
#   - the folder above the file list reads "C:\Test"
#   - the combo box beside it is showing "minimal.toml"
#   - the navigation tree carries "General", "Keymaps" and "notepad"
#   - selecting "notepad" brings up "notepad.exe", "C-h" and "Back"
#
# The last one is the point of the whole check: those three strings are what
# minimal.toml says, so seeing them is what proves the window is reading the
# file rather than showing a shape.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled.

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\winapp-config-display.txt"

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

Say ""
Say "=== opening the settings window ==="
Start-App "C:\Test\minimal.toml" | Out-Null
Check "app-running" ([bool](Get-Process winremap -ErrorAction SilentlyContinue)) "winremap.exe is resident"
Check "settings-opens" (Open-Settings) "tray menu -> invoke 1003"
$settings = WinappWindow "winremap" "*ettings*"
Check "settings-window-listed" ([bool]$settings) $(if ($settings) { "'" + $settings.title + "'" } else { "not listed" })
if (-not $settings) { Flush; exit 1 }
$target = @("-w", "$($settings.hwnd)")

# =========================================================================
# which file is loaded
# =========================================================================
Say ""
Say "=== the loaded config ==="
$els = WinappElements $target
Say ("elements: " + $els.Count)

# The folder is a plain text element; the file name is the combo box's value,
# and a value is not a name - `inspect` does not carry it, so the combo is
# asked for its properties separately.
$folder = WinappFind $els "C:\Test"
Check "config-folder" ($folder.Count -ge 1) "$($folder.Count) element(s) named 'C:\Test'"

$combos = @($els | Where-Object { (WinappType $_) -match "(?i)combo" })
Say ("combo boxes: " + $combos.Count)
$comboValue = ""
foreach ($c in $combos) {
    # Dumped as well as read: which field carries the displayed text is
    # winapp's business, and a check that silently asserts against an always-
    # empty field passes for the wrong reason the day the field moves.
    $props = WinappProperties $c.selector $target
    if ($props) { foreach ($p in $props.PSObject.Properties) { Say ("  " + $c.selector + " ." + $p.Name + " = " + $p.Value) } }
    else { Say ("  " + $c.selector + " : get-property returned nothing") }
    $comboValue = WinappProperty $c.selector "Value" $target
    if ($comboValue) { break }
}
Check "config-file-name" ($comboValue -eq "minimal.toml") "the combo box is showing '$comboValue'"

# =========================================================================
# the navigation tree
# =========================================================================
Say ""
Say "=== navigation ==="
foreach ($name in @("General", "Keymaps", "notepad")) {
    $hit = WinappFind $els $name
    Check ("nav-" + $name.ToLower()) ($hit.Count -ge 1) "$($hit.Count) element(s) named '$name'"
}

# =========================================================================
# selecting a keymap shows its rules
# =========================================================================
Say ""
Say "=== selecting the 'notepad' keymap ==="
$how = WinappActivate "notepad" $target
Say ("activated by: " + $(if ($how) { $how } else { "NOTHING WORKED" }))
Check "keymap-selectable" ([bool]$how) "the tree item responded to invoke or to a click on its slug"

# The rule table appears in place of the placeholder, so waiting on one of its
# own strings is what says the transition finished - no fixed sleep.
$arrived = WinappWaitFor "notepad.exe" $target 8000
Check "rules-appear" $arrived "wait-for saw 'notepad.exe' after selecting the keymap"

$after = WinappElements $target
Say ("elements after selecting: " + $after.Count)
foreach ($e in $after) { Say ("  " + (WinappType $e) + " '" + (WinappName $e) + "' selector=" + $e.selector) }

# What minimal.toml says, read back off the screen.
foreach ($name in @("notepad.exe", "C-h", "Back")) {
    $hit = WinappFind $after $name
    Check ("rule-" + ($name -replace '[^A-Za-z]', '').ToLower()) ($hit.Count -ge 1) "$($hit.Count) element(s) named '$name'"
}

# The assertion has to be able to fail.
$absent = WinappFind $after "C-q"
Check "absent-rule" ($absent.Count -eq 0) "a rule the config does not contain matches $($absent.Count) elements"

Say ""
Say ("=== wall clock: {0:mm\:ss} ===" -f $total.Elapsed)
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
