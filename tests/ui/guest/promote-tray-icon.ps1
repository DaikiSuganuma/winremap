# Runs inside the guest, before the tray scenarios.
#
# Windows 11 hides new notification-area icons in the overflow flyout, and
# opening that flyout through UI automation is where the test agent stalls —
# it hovers the chevron for minutes without the flyout ever opening. Windows
# keeps a per-icon "show on the taskbar" flag (IsPromoted), so setting it puts
# WinRemap's icon directly on the taskbar and the scenarios can right-click it.
#
# The entry only exists once the icon has been registered at least once, hence
# the throwaway launch below.

$ErrorActionPreference = "Stop"
$exe = "C:\Test\winremap.exe"

Start-Process $exe -ArgumentList "--config", "C:\Test\minimal.toml", "--lang", "en"
Start-Sleep -Seconds 8

$root = "HKCU:\Control Panel\NotifyIconSettings"
$promoted = 0
if (Test-Path $root) {
    foreach ($key in Get-ChildItem $root) {
        $props = Get-ItemProperty $key.PSPath
        if ($props.PSObject.Properties.Name -contains "ExecutablePath" -and
            $props.ExecutablePath -like "*winremap.exe") {
            Set-ItemProperty $key.PSPath -Name IsPromoted -Value 1 -Type DWord
            $promoted++
        }
    }
}
Write-Output "promoted entries: $promoted"

# The flag is read when the icon is registered, so the instance that created
# the entry has to go and the shell has to reload before the scenario starts.
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force
Stop-Process -Name explorer -Force   # the shell restarts itself
Start-Sleep -Seconds 12
