# Dot-sourced by the guest-side check scripts. Everything here is about
# reaching WinRemap from outside it: finding its windows and its tray icon,
# opening the tray menu, pressing what UI Automation exposes.
#
# The caller provides Say([string]) before dot-sourcing this; these helpers
# report what they could not do rather than throwing, because a check that
# dies silently is indistinguishable from one that never ran.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled.

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Nat {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string n);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public const uint RIGHTDOWN = 0x0008, RIGHTUP = 0x0010;
  public static void Key(byte vk) {
    keybd_event(vk, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(60);
    keybd_event(vk, 0, 2, IntPtr.Zero);
    System.Threading.Thread.Sleep(250);
  }
  // Held modifier plus key, which is what a remap rule matches on.
  public static void Chord(byte mod, byte vk) {
    keybd_event(mod, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(40);
    keybd_event(vk, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(60);
    keybd_event(vk, 0, 2, IntPtr.Zero);
    System.Threading.Thread.Sleep(40);
    keybd_event(mod, 0, 2, IntPtr.Zero);
    System.Threading.Thread.Sleep(250);
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)]
  public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int size);
  // Same event either way; what differs is whether the scan code is filled in,
  // which some hooks and some remappers key off.
  static void Send(ushort vk, ushort scan, uint flags) {
    var i = new INPUT();
    i.type = 1;
    i.ki.wVk = vk;
    i.ki.wScan = scan;
    i.ki.dwFlags = flags;
    SendInput(1, new INPUT[] { i }, Marshal.SizeOf(typeof(INPUT)));
  }
  public static void SendInputKey(ushort vk) {
    Send(vk, 0, 0);
    System.Threading.Thread.Sleep(60);
    Send(vk, 0, 2);
    System.Threading.Thread.Sleep(250);
  }
  public static void SendInputScan(ushort vk, ushort scan) {
    Send(vk, scan, 0);
    System.Threading.Thread.Sleep(60);
    Send(vk, scan, 2);
    System.Threading.Thread.Sleep(250);
  }
  public static void RightClick(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(300);
    mouse_event(RIGHTDOWN, 0, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(80);
    mouse_event(RIGHTUP, 0, 0, 0, IntPtr.Zero);
  }
}
'@

$root = [System.Windows.Automation.AutomationElement]::RootElement
$desc = [System.Windows.Automation.TreeScope]::Descendants
$kids = [System.Windows.Automation.TreeScope]::Children
$any = [System.Windows.Automation.Condition]::TrueCondition
$textType = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Text)

# The notification-area icon - and not the taskbar button of an open window,
# which is also a Button whose name contains "WinRemap". Right-clicking that
# one opens a jump list instead of the tray menu, and every check that needs
# the menu then fails while the app is fine. The icon's name is its tooltip
# ("WinRemap - 1 keymap(s)"), never exactly a window title, so the open
# windows' titles are what tells them apart in any display language.
function Find-Icon {
    $btn = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Button)
    foreach ($e in $root.FindAll($desc, $btn)) {
        $n = $e.Current.Name
        if ($n -notlike "*WinRemap*") { continue }
        # A taskbar button carries the launcher's AppUserModelID as its
        # AutomationId and is named after the window plus a count in the
        # display language ("WinRemap - log - 1 running window"), so neither the
        # name nor a title comparison separates them reliably. The id does.
        if ("$($e.Current.AutomationId)" -like "Appid:*") { continue }
        return $e
    }
    return $null
}

# Every top-level window of every winremap process, by title. The count is the
# check for "did not open twice" and "nothing was left behind".
function Get-AppWindows {
    $found = @()
    foreach ($p in @(Get-Process winremap -ErrorAction SilentlyContinue)) {
        $cond = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
        foreach ($w in $root.FindAll($kids, $cond)) { $found += $w }
    }
    return $found
}
function Get-WindowLike([string]$TitleLike) {
    foreach ($w in Get-AppWindows) { if ($w.Current.Name -like $TitleLike) { return $w } }
    return $null
}
function Count-WindowsLike([string]$TitleLike) {
    $n = 0
    foreach ($w in Get-AppWindows) { if ($w.Current.Name -like $TitleLike) { $n++ } }
    return $n
}
function Get-Texts($window) {
    $out = @()
    if (-not $window) { return $out }
    foreach ($t in $window.FindAll($desc, $textType)) { if ($t.Current.Name) { $out += $t.Current.Name } }
    return $out
}
function Find-Named($parent, [string]$name) {
    if (-not $parent) { return $null }
    foreach ($e in $parent.FindAll($desc, $any)) { if ($e.Current.Name -eq $name) { return $e } }
    return $null
}
function Click-Element($e) {
    $r = $e.Current.BoundingRectangle
    [Nat]::SetCursorPos([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2)) | Out-Null
    Start-Sleep -Milliseconds 200
    [Nat]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [Nat]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Seconds 2
}
# InvokePattern where the element offers it, a real click where it does not
# (egui's tree items expose no Invoke).
function Invoke-Element($e) {
    try {
        $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Seconds 1
        return $true
    }
    catch { Click-Element $e; return $true }
}

# The tray menu is a Win32 popup (class #32768) and its items are not in the
# UIA tree, so this is in two halves:
#
#   opening it   - a real right-click on the icon. Retried, because a click
#                  that lands while the shell is busy simply does nothing and
#                  the menu never appears (it did, in 4 of 14 checks).
#   choosing an  - winapp's `invoke <command id>`. The first-letter trick this
#   item           used to use is tied to the display language: with --lang ja
#                  the item labels are Japanese, "S" matches nothing, and the
#                  check fails while the app is perfectly fine.
#
# Command ids come from the app's own menu (tests/ui/guest/probe-winapp.ps1):
# 1001 Enabled, 1002 Reload config, 1003 Settings, 1004 Show log, 1005 Quit.
$env:Path = $env:Path + ';' + $env:LOCALAPPDATA + '\Microsoft\WindowsApps'

function Open-TrayMenu {
    foreach ($attempt in 1..3) {
        # Esc first: a menu left open from the previous step swallows the click.
        [Nat]::Key(0x1B)
        Start-Sleep -Seconds 1
        $icon = Find-Icon
        if (-not $icon) { Say "  tray icon not found (attempt $attempt)"; Start-Sleep -Seconds 3; continue }
        $r = $icon.Current.BoundingRectangle
        [Nat]::RightClick([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
        for ($i = 0; $i -lt 10; $i++) {
            $h = [Nat]::FindWindow("#32768", $null)
            if ($h -ne [IntPtr]::Zero) { return $h }
            Start-Sleep -Milliseconds 500
        }
        Say "  tray menu did not open (attempt $attempt)"
    }
    return [IntPtr]::Zero
}

function Invoke-MenuItem([int]$Id) {
    $h = Open-TrayMenu
    if ($h -eq [IntPtr]::Zero) { return $false }
    $out = (& winapp ui invoke "$Id" -w "$([int64]$h)" 2>&1 | Out-String)
    $ok = ($LASTEXITCODE -eq 0)
    if (-not $ok) { Say ("  invoke $Id failed: " + ($out.Trim() -replace "\r?\n", " / ")) }
    Start-Sleep -Seconds 3
    return $ok
}
function Open-Settings { return (Invoke-MenuItem 1003) }
function Open-Log { return (Invoke-MenuItem 1004) }
function Toggle-Enabled { return (Invoke-MenuItem 1001) }
function Reload-Config { return (Invoke-MenuItem 1002) }
function Quit-App { return (Invoke-MenuItem 1005) }

function Close-WindowLike([string]$TitleLike) {
    $w = Get-WindowLike $TitleLike
    if (-not $w) { return $false }
    try {
        $w.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        Start-Sleep -Seconds 2
        return $true
    }
    catch { Say ("  close failed: " + $_); return $false }
}

# Puts a window in front and says whether it worked. SetForegroundWindow alone
# is not enough: a process with no window of its own has no foreground right,
# so Windows quietly demotes the call to a taskbar flash. A real click has no
# such rule, which is why the fallback is a click - and why the result is
# verified rather than assumed. A check that types into the wrong window
# reports the application as broken.
function Focus-Window($w) {
    if (-not $w) { return $false }
    $h = [IntPtr]$w.Current.NativeWindowHandle
    foreach ($attempt in 1..3) {
        [Nat]::SetForegroundWindow($h) | Out-Null
        Start-Sleep -Milliseconds 600
        if ([Nat]::GetForegroundWindow() -eq $h) { return $true }
        Click-Element $w
        if ([Nat]::GetForegroundWindow() -eq $h) { return $true }
        Say ("  focus attempt $attempt did not take")
    }
    return $false
}

function Start-App([string]$Config, [string]$Lang = "en", [string[]]$Extra = @()) {
    Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    $t = Get-Date
    $argv = @('--config', $Config, '--lang', $Lang) + $Extra
    Start-Process $exe -ArgumentList $argv
    Start-Sleep -Seconds 6
    return $t
}
