# Runs inside the guest, in session 1. The v0.1-v0.3 acceptance items that a
# machine can judge: window lifecycle, the operation log, and what the settings
# window shows for a config with more than one keymap.
#
# See docs/v0.5/03_acceptance-checklist.md - section 3 says which item each
# check stands in for, and section 4 is what is left for a person to do.
#
# No AI in this loop, for the same reason as dump-uia.ps1: an agent that cannot
# find something goes looking elsewhere and reports on that instead, which
# makes "the window is wrong" and "the agent never opened it" identical.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled; where Japanese matters (the
# --lang ja check) the test is "there are characters outside Latin-1", not a
# literal comparison.

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\regression-checks.txt"

$lines = New-Object System.Collections.Generic.List[string]
$checks = [ordered]@{}
function Say([string]$s) { $lines.Add($s) }
# $Ok is deliberately untyped: `-match` against an array returns the matching
# elements, not a boolean, and a [bool] parameter throws mid-run - losing every
# check after it. Anything truthy counts as a pass, an empty result does not.
function Check([string]$name, $Ok, [string]$detail) {
    $ok = if ($Ok -is [bool]) { $Ok } else { @($Ok | Where-Object { $_ }).Count -gt 0 }
    $checks[$name] = $ok
    Say ("CHECK {0,-30} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}
function Flush { $lines | Set-Content $outPath -Encoding UTF8 }
trap { Say "EXCEPTION: $_"; Say $_.ScriptStackTrace; Flush; exit 1 }

# Finding windows, the tray icon and the menu is the same job in every guest
# script, so it lives in one file (they are dot-sourced, so Say is in scope).
. "$PSScriptRoot\ui-helpers.ps1"

Say ("powershell " + $PSVersionTable.PSVersion)
Say ("started " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))

# =========================================================================
# 1. the log window: banner, clipboard, reopening, tray actions
# =========================================================================
Say ""
Say "=== log window ==="
$launched = Start-App "C:\Test\minimal.toml"
# Deliberately late: the banner must carry the time the session started, not
# the time the window was opened (v0.2 B1-29).
Start-Sleep -Seconds 20
Open-Log | Out-Null
$log = Get-WindowLike "*log*"
$texts = Get-Texts $log
Say ("log window: " + $(if ($log) { "'" + $log.Current.Name + "'" } else { "NOT OPEN" }) + "  text elements: " + $texts.Count)
foreach ($t in $texts) { Say ("  | " + $t) }

$banner = $texts | Where-Object { $_ -match '^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}' } | Select-Object -First 1
$bannerTime = $null
if ($banner -and $banner -match '^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})') {
    $bannerTime = [datetime]::ParseExact($Matches[1], "yyyy-MM-dd HH:mm:ss", $null)
}
$sinceLaunch = if ($bannerTime) { [Math]::Abs(($bannerTime - $launched).TotalSeconds) } else { -1 }
Say ("banner='" + $banner + "'  seconds from launch: " + $sinceLaunch + " (window was opened ~20s after launch)")
Check "banner-shows-start-time" ($bannerTime -and $sinceLaunch -le 10) `
    "the first log line carries the session start, not the time the window opened (v0.2 B1-29)"

$copy = Find-Named $log "Copy all"
if ($copy) {
    Set-Clipboard -Value "cleared-before-the-check"
    Invoke-Element $copy | Out-Null
    Start-Sleep -Seconds 2
}
[string]$clip = (Get-Clipboard) -join "`n"
Check "copy-all-fills-clipboard" ([bool]$copy -and $clip -and $clip -match 'WinRemap v\d') `
    "Copy all puts the log on the clipboard (v0.2 A-15)"

# Choosing "Show log" while it is open must raise the one window, not open a
# second (v0.2 A-17b); three rounds of open/close must not break the event
# loop (A-17, B0-9).
Open-Log | Out-Null
$dupes = Count-WindowsLike "*log*"
Check "log-does-not-open-twice" ($dupes -eq 1) "$dupes log window(s) after asking twice (v0.2 A-17b)"

$cycles = 0
foreach ($i in 1..3) {
    if (Close-WindowLike "*log*") { if (Open-Log) { if (Get-WindowLike "*log*") { $cycles++ } } }
}
Check "log-reopens-three-times" ($cycles -eq 3) `
    "$cycles of 3 close/open rounds worked - no EventLoop can't be recreated (v0.2 A-17, B0-9)"

# Tray actions must leave a line behind (v0.2 B-L1, A-18, A-28).
$before = (Get-Texts (Get-WindowLike "*log*")).Count
Toggle-Enabled | Out-Null
Toggle-Enabled | Out-Null
Reload-Config | Out-Null
$after = Get-Texts (Get-WindowLike "*log*")
$actions = @($after | Where-Object { $_ -match '\[action\]' })
Say ("log lines before=" + $before + " after=" + $after.Count + " with [action]=" + $actions.Count)
foreach ($a in $actions) { Say ("  > " + $a) }
Check "tray-actions-are-logged" ($actions.Count -ge 3) `
    "toggling twice and reloading wrote [action] lines (v0.2 B-L1, A-18, A-28)"

# =========================================================================
# 2. two windows, and closing one
# =========================================================================
Say ""
Say "=== both windows ==="
Open-Settings | Out-Null
$bothOpen = (Count-WindowsLike "*log*") -eq 1 -and (Count-WindowsLike "*settings*") -eq 1
Check "both-windows-open" $bothOpen "the settings and log windows are up at the same time (v0.2 B0-5)"

# The settings window owns "Open in text editor"; pressing it must start the
# associated editor (v0.2 B0-3, B0-15).
# v0.2 B0-3 / B0-15 ("open in text editor") are NOT checked here: the button
# does not exist any more. v0.4 replaced it with editing inside the window, and
# the header now carries the folder, the file and an Edit button instead. The
# element dump that showed this is in docs/v0.5/03_acceptance-checklist.md §3.2,
# where both items are recorded as obsolete.
$settings = Get-WindowLike "*settings*"
$headerNames = @()
if ($settings) { foreach ($e in $settings.FindAll($desc, $any)) { if ($e.Current.Name) { $headerNames += $e.Current.Name } } }
Check "config-path-in-header" (($headerNames -contains "C:\Test") -and ($headerNames -contains "Edit")) `
    "the header names the config folder and offers Edit (v0.4's replacement for B0-3)"

Close-WindowLike "*settings*" | Out-Null
$logAlive = (Count-WindowsLike "*log*") -eq 1
$settingsGone = (Count-WindowsLike "*settings*") -eq 0
Check "closing-settings-keeps-log" ($logAlive -and $settingsGone) `
    "closing the settings window leaves the log window running (v0.2 B0-6)"

# Quitting from the tray with a window open must take everything with it
# (v0.2 A-19, B0-10).
Quit-App | Out-Null
Start-Sleep -Seconds 4
$stillRunning = [bool](Get-Process winremap -ErrorAction SilentlyContinue)
Check "quit-closes-everything" (-not $stillRunning) "no winremap process left (v0.2 A-19, B0-10)"

# =========================================================================
# 3. opening only the log must not create the settings window
# =========================================================================
Say ""
Say "=== log only ==="
Start-App "C:\Test\minimal.toml" | Out-Null
Open-Log | Out-Null
$logOnly = (Count-WindowsLike "*log*") -eq 1 -and (Count-WindowsLike "*settings*") -eq 0
Check "log-only-keeps-settings-closed" $logOnly `
    "the settings viewport is never created (v0.2 B0-8, B0-12)"

# =========================================================================
# 4. a config with several keymaps, macros and a [macro] section
# =========================================================================
Say ""
Say "=== rich config ==="
Start-App "C:\Test\personal-ja.toml" | Out-Null
Open-Settings | Out-Null
$settings = Get-WindowLike "*settings*"
$names = @()
if ($settings) { foreach ($e in $settings.FindAll($desc, $any)) { if ($e.Current.Name) { $names += $e.Current.Name } } }
Say ("settings elements: " + $names.Count)
Check "rich-config-lists-keymaps" ($names -contains "browser-native-keys" -and $names -contains "acrobat-native-keys") `
    "every keymap of the file is in the navigation tree (v0.2 B1-1, B1-3)"

# The [macro] section names the recording keys; they are shown as configured
# (v0.3 M-1, M-70).
$hasRecordKeys = ($names -contains "S-F10") -and ($names -contains "F10")
Check "macro-section-shows-keys" $hasRecordKeys "S-F10 and F10 are shown (v0.3 M-1, M-70)"

# Nothing has been recorded, and nothing may be written to disk for it
# (v0.3 M-63, M-64 - invariant 6).
$appData = Join-Path $env:APPDATA "winremap"
$onDisk = if (Test-Path $appData) { @(Get-ChildItem $appData -Recurse -File).Count } else { 0 }
[string]$configText = (Get-Content "C:\Test\personal-ja.toml") -join "`n"
Check "recording-not-persisted" ($onDisk -eq 0 -and $configText -notmatch "recorded") `
    "$onDisk files under %APPDATA%\winremap and nothing added to the config (v0.3 M-63, M-64)"

# =========================================================================
# 5. the interface follows --lang
# =========================================================================
Say ""
Say "=== --lang ja ==="
Start-App "C:\Test\minimal.toml" "ja" | Out-Null
Open-Settings | Out-Null
# The window's own title is Japanese now, so collect from every window the app
# has rather than matching a title this script cannot spell in ASCII.
$jaNames = @()
foreach ($w in Get-AppWindows) {
    foreach ($e in $w.FindAll($desc, $any)) { if ($e.Current.Name) { $jaNames += $e.Current.Name } }
}
$nonLatin = @($jaNames | Where-Object { $_ -match '[^\u0000-\u00FF]' })
Say ("elements: " + $jaNames.Count + "  with characters outside Latin-1: " + $nonLatin.Count)
Check "japanese-ui-switches" ($nonLatin.Count -gt 0 -and ($jaNames -notcontains "Keymaps")) `
    "--lang ja replaces the English labels (v0.3 M-82)"

Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
