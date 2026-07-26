# Runs inside the guest, in session 1. Opens WinRemap's settings and log
# windows through the tray menu and writes what they expose to UI Automation
# to C:\Test\uia-dump.txt.
#
# There is deliberately no AI in this loop. The scenarios are driven by an
# agent, and an agent that cannot find something tends to go looking for it
# somewhere else and report on that instead — which is how "the settings
# window exposes nothing" and "the agent never opened the settings window"
# came to look identical. This script is what the selectors baked into the
# scenarios are read from, and what to re-run when the UI changes.
#
# Host side: .\run-vm-ui-test.ps1 -DumpUia

$ErrorActionPreference = "Stop"

$out = New-Object System.Collections.Generic.List[string]
function Say([string]$s) { $out.Add($s) }
$dumpPath = "C:\Test\uia-dump.txt"

# Pressing is checked here rather than in the agent-driven scenarios. Through
# terminator the same two buttons worked about half the time — "Edit" 2 of 6,
# "Clear" 2 of 4 — while a plain UIA client has yet to miss. A check that
# fails half the time cannot tell a regression from a bad day, so the agent
# reads and this asserts.
$checks = [ordered]@{}
function Check([string]$name, [bool]$ok, [string]$detail) {
    $checks[$name] = $ok
    Say ("CHECK {0,-28} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}

# Whatever happens, the host gets a file: vmrun does not carry guest stdout
# back, so a script that dies silently here is indistinguishable from one that
# never ran at all.
trap {
    $out.Add("EXCEPTION: $_")
    $out.Add($_.ScriptStackTrace)
    $out | Set-Content $dumpPath -Encoding UTF8
    exit 1
}

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Say ("powershell " + $PSVersionTable.PSVersion)

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Native {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string n);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
  public const uint LEFTDOWN = 0x0002, LEFTUP = 0x0004, RIGHTDOWN = 0x0008, RIGHTUP = 0x0010;
  public static void Key(byte vk) {
    keybd_event(vk, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(60);
    keybd_event(vk, 0, 2, IntPtr.Zero);
    System.Threading.Thread.Sleep(250);
  }
  public static void Click(int x, int y, uint down, uint up) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(300);
    mouse_event(down, 0, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(80);
    mouse_event(up, 0, 0, 0, IntPtr.Zero);
  }
}
'@

$root = [System.Windows.Automation.AutomationElement]::RootElement
$desc = [System.Windows.Automation.TreeScope]::Descendants
$anything = [System.Windows.Automation.Condition]::TrueCondition

function Find-ByNameLike([System.Windows.Automation.AutomationElement]$parent, [string]$like, [string]$type) {
    $cond = if ($type) {
        New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::$type)
    }
    else { $anything }
    foreach ($e in $parent.FindAll($desc, $cond)) {
        if ($e.Current.Name -like $like) { return $e }
    }
    return $null
}

# GetClickablePoint() throws on the notification-area button ("no clickable
# point"), so take the middle of the bounding rectangle instead.
function Get-Center([System.Windows.Automation.AutomationElement]$e) {
    $r = $e.Current.BoundingRectangle
    return @([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
}

# The tray menu is a Win32 popup (class #32768), and this UIA client sees it as
# an empty Pane — the items are not in the tree at all. That is a property of
# Windows menus, not of the windows under test, so drive it the way Windows
# itself offers: first letter selects, Enter invokes. "Settings" and "Show log"
# share an S, so one press highlights the first and two the second; with more
# than one match Windows highlights without invoking.
function Open-TrayMenuItem([System.Windows.Automation.AutomationElement]$icon, [string]$label, [byte]$vk, [int]$presses) {
    $pt = Get-Center $icon
    [Native]::Click($pt[0], $pt[1], [Native]::RIGHTDOWN, [Native]::RIGHTUP)
    Start-Sleep -Seconds 2
    if ([Native]::FindWindow("#32768", $null) -eq [IntPtr]::Zero) {
        Say "tray menu did not open"
        return $false
    }
    for ($i = 0; $i -lt $presses; $i++) { [Native]::Key($vk) }
    [Native]::Key(0x0D)  # Enter
    Say ("invoked tray menu item: " + $label)
    return $true
}

function Write-WindowTree([string]$titleLike, [string]$label) {
    Say ""
    Say "=== $label (title like '$titleLike') ==="
    $win = $null
    for ($i = 0; $i -lt 10; $i++) {
        $win = Find-ByNameLike $root $titleLike "Window"
        if ($win) { break }
        Start-Sleep -Seconds 1
    }
    if (-not $win) { Say "WINDOW NOT FOUND"; return $null }
    Say ("window: '" + $win.Current.Name + "'")
    $all = $win.FindAll($desc, $anything)
    Say ("descendants: " + $all.Count)
    $n = 0
    foreach ($e in $all) {
        $n++
        if ($n -gt 120) { Say "  ... (truncated)"; break }
        $c = $e.Current
        $line = "  [{0}] {1} '{2}'" -f $n, $c.ControlType.ProgrammaticName.Replace("ControlType.", ""), $c.Name
        if ($c.AutomationId) { $line += " id=" + $c.AutomationId }
        $pats = @()
        foreach ($p in $e.GetSupportedPatterns()) { $pats += $p.ProgrammaticName.Replace("PatternIdentifiers.Pattern", "") }
        if ($pats.Count) { $line += " patterns=" + ($pats -join ",") }
        if ($pats -contains "Value") {
            $line += " value='" + $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value + "'"
        }
        if ($pats -contains "Toggle") {
            $line += " state=" + $e.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState
        }
        Say $line
    }
    return $win
}

function Invoke-Element([System.Windows.Automation.AutomationElement]$e) {
    try {
        $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    }
    catch {
        $pt = Get-Center $e
        [Native]::Click($pt[0], $pt[1], [Native]::LEFTDOWN, [Native]::LEFTUP)
    }
}

function Find-Named([System.Windows.Automation.AutomationElement]$parent, [string]$name) {
    foreach ($e in $parent.FindAll($desc, $anything)) {
        if ($e.Current.Name -eq $name) { return $e }
    }
    return $null
}

# --- launch -------------------------------------------------------------
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
Start-Process C:\Test\winremap.exe -ArgumentList '--config', 'C:\Test\minimal.toml', '--lang', 'en'
Start-Sleep -Seconds 6
Say ("winremap running: " + [bool](Get-Process winremap -ErrorAction SilentlyContinue))

$icon = Find-ByNameLike $root "*WinRemap*" "Button"
if (-not $icon) {
    Say "TRAY ICON NOT FOUND"
    $out | Set-Content $dumpPath -Encoding UTF8
    exit 1
}
Say ("tray icon: '" + $icon.Current.Name + "' class=" + $icon.Current.ClassName)

# --- settings window ----------------------------------------------------
Open-TrayMenuItem $icon "Settings" 0x53 1 | Out-Null
Start-Sleep -Seconds 4
$settings = Write-WindowTree "*settings*" "SETTINGS WINDOW"
Check "settings-window-exposed" ([bool]$settings -and $settings.FindAll($desc, $anything).Count -gt 20) `
    "the settings window's own controls reach UI Automation, not just its title bar"

if ($settings) {
    $node = Find-Named $settings "notepad"
    if ($node) {
        Say ""
        Say "--- selecting tree item 'notepad' ---"
        Invoke-Element $node
        Start-Sleep -Seconds 3
        Write-WindowTree "*settings*" "SETTINGS WINDOW after selecting notepad" | Out-Null
    }
    else { Say "tree item 'notepad' not found" }
    Check "keymap-detail-shown" ([bool](Find-Named $settings "notepad.exe") -and [bool](Find-Named $settings "C-h")) `
        "selecting the keymap shows its application and rule"

    # Pressing a control is the half this cannot cover by reading, so press one
    # and require the window to change.
    $edit = Find-Named $settings "Edit"
    if ($edit) {
        Say ""
        Say "--- invoking Button 'Edit' through UIA ---"
        Invoke-Element $edit
        Start-Sleep -Seconds 3
        Write-WindowTree "*settings*" "SETTINGS WINDOW in edit mode" | Out-Null
    }
    else { Say "Button 'Edit' not found" }
    Check "edit-button-presses" ([bool](Find-Named $settings "Save") -and [bool](Find-Named $settings "Revert")) `
        "pressing Edit puts the window in edit mode"
}

# --- log window ---------------------------------------------------------
Open-TrayMenuItem $icon "Show log" 0x53 2 | Out-Null
Start-Sleep -Seconds 4
$log = Write-WindowTree "*log*" "LOG WINDOW"
$textCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Text)
Check "log-lines-readable" ([bool]$log -and $log.FindAll($desc, $textCond).Count -gt 0) `
    "the log lines are readable as elements"

if ($log) {
    $clear = Find-Named $log "Clear"
    if ($clear) {
        Say ""
        Say "--- invoking Button 'Clear' through UIA ---"
        Invoke-Element $clear
        Start-Sleep -Seconds 3
        Write-WindowTree "*log*" "LOG WINDOW after Clear" | Out-Null
    }
    else { Say "Button 'Clear' not found" }
    Check "clear-button-presses" ($log.FindAll($desc, $textCond).Count -eq 0) `
        "pressing Clear empties the log"
}

$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
$out | Set-Content $dumpPath -Encoding UTF8
if ($failed.Count) { exit 1 }
exit 0
