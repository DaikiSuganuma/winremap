# Runs inside the guest, before the tray scenarios.
#
# Windows 11 hides new notification-area icons in the overflow flyout, and
# opening that flyout through UI automation is where the test agent stalls —
# it hovers the chevron for minutes without the flyout ever opening. Windows
# keeps a per-icon "show on the taskbar" flag (IsPromoted), so setting it puts
# WinRemap's icon directly on the taskbar and the scenarios can right-click it.
#
# The registry entry only exists once the icon has been registered at least
# once, hence the throwaway launch below. How long that takes varies with how
# busy the freshly booted guest is, so wait for the entry rather than guess:
# an unpromoted icon costs a scenario its whole timeout while the agent hunts
# for something that is sitting in the overflow.
#
# The result goes to a file because vmrun does not carry guest stdout back to
# the host — without it, a silent no-op here looks like a bug in the app.

$ErrorActionPreference = "Stop"
$exe = "C:\Test\winremap.exe"
$resultPath = "C:\Test\promote-result.txt"
$root = "HKCU:\Control Panel\NotifyIconSettings"

Remove-Item $resultPath -Force -ErrorAction SilentlyContinue
Start-Process $exe -ArgumentList "--config", "C:\Test\minimal.toml", "--lang", "en"

function Get-WinRemapEntries {
    if (-not (Test-Path $root)) { return @() }
    @(Get-ChildItem $root | Where-Object {
            $props = Get-ItemProperty $_.PSPath
            $props.PSObject.Properties.Name -contains "ExecutablePath" -and
            $props.ExecutablePath -like "*winremap.exe"
        })
}

$deadline = (Get-Date).AddSeconds(60)
$entries = @()
while ((Get-Date) -lt $deadline) {
    $entries = Get-WinRemapEntries
    if ($entries.Count -gt 0) { break }
    Start-Sleep -Seconds 2
}

foreach ($entry in $entries) {
    Set-ItemProperty $entry.PSPath -Name IsPromoted -Value 1 -Type DWord
}

# Only the instance that created the entry has to go — the scenario's own
# launch picks the flag up. Explorer is deliberately left alone: restarting it
# made the icon fail to register at all in about half the runs.
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 3

Set-Content -Path $resultPath -Value "promoted=$($entries.Count)" -Encoding ASCII
