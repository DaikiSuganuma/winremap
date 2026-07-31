# Runs inside the guest, in session 1. Scenario 04-log-window with the agent
# taken out of the loop: the log window opens from the tray menu, carries a
# session line naming the version, and offers its controls.
#
# What the agent version asserted, and this one keeps:
#   - a text element holding a session line: a timestamp, then "WinRemap v..."
#   - a check box named "Follow newest"
#   - buttons named "Clear" and "Copy all"
#
# Pressing them is deliberately NOT part of this check; 00-uia-actuation covers
# actuation. Which config file was loaded is still not written to this window -
# its absence is not a failure (docs/05_ui-test-automation.md).
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled.

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\winapp-log-window.txt"

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
Say "=== opening the log window ==="
Start-App "C:\Test\minimal.toml" | Out-Null
Check "app-running" ([bool](Get-Process winremap -ErrorAction SilentlyContinue)) "winremap.exe is resident"
Check "log-opens" (Open-Log) "tray menu -> invoke 1004 (Show log)"

# "*log*" would also match the settings window on a display language whose word
# for settings contains those letters; the English UI is fixed by --lang en, and
# the log window is the only one titled with "log" there.
$logw = WinappWindow "winremap" "*log*"
Check "log-window-listed" ([bool]$logw) $(if ($logw) { "'" + $logw.title + "'" } else { "no window whose title contains 'log'" })
if (-not $logw) { Flush; exit 1 }
$target = @("-w", "$($logw.hwnd)")

Say ""
Say "=== elements ==="
$els = WinappElements $target
Say ("elements: " + $els.Count)
foreach ($e in $els) { Say ("  " + (WinappType $e) + " '" + (WinappName $e) + "' selector=" + $e.selector) }

# =========================================================================
# the log lines
# =========================================================================
Say ""
Say "=== log lines ==="
$texts = @($els | Where-Object { (WinappType $_) -match "(?i)text" })
foreach ($t in $texts) { Say ("  | " + (WinappName $t)) }
Check "log-has-lines" ($texts.Count -ge 1) "$($texts.Count) text elements"

# The session banner: a timestamp followed by the version. Matched as a shape
# rather than as a literal, because the time is different every run and the
# version changes every release - pinning either would make this check a
# maintenance task rather than a test.
$banner = @($texts | Where-Object { (WinappName $_) -match '^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}.*WinRemap v\d' })
Check "session-line" ($banner.Count -ge 1) `
    $(if ($banner.Count) { "'" + (WinappName $banner[0]) + "'" } else { "no line matching '<timestamp> ... WinRemap v<n>'" })

# =========================================================================
# the controls
# =========================================================================
Say ""
Say "=== controls ==="
$follow = WinappFind $els "Follow newest"
Check "follow-checkbox" ($follow.Count -eq 1) "$($follow.Count) element(s) named 'Follow newest'"
if ($follow.Count -eq 1) {
    $props = WinappProperties $follow[0].selector $target
    if ($props) { foreach ($p in $props.PSObject.Properties) { Say ("  Follow newest ." + $p.Name + " = " + $p.Value) } }
    else { Say "  Follow newest: get-property returned nothing" }
}

foreach ($name in @("Clear", "Copy all")) {
    $hit = WinappFind $els $name "Button"
    Check ("button-" + ($name -replace '\s', '').ToLower()) ($hit.Count -eq 1) "$($hit.Count) Button named '$name'"
}

# The assertion has to be able to fail.
$absent = WinappFind $els "NoSuchControlHere"
Check "absent-control" ($absent.Count -eq 0) "a name nothing carries matches $($absent.Count) elements"

Say ""
Say ("=== wall clock: {0:mm\:ss} ===" -f $total.Elapsed)
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
