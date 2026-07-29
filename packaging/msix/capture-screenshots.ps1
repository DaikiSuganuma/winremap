<#
.SYNOPSIS
    Captures the Store listing screenshots (v0.6 plan Phase B).

.DESCRIPTION
    Grabs WinRemap's own windows and composites each onto a plain canvas, so
    nothing else on the machine can end up in a published image. A full-screen
    capture would carry whatever happens to be open; these images contain the
    application window and nothing else.

    The canvas is 1920x1080 because the Store wants at least 1366x768 and
    WinRemap's windows are smaller than that on their own.

    Run once per listing language:

        .\capture-screenshots.ps1 -Lang en
        .\capture-screenshots.ps1 -Lang ja

    Images land in packaging\msix\screenshots\ (not committed — they are
    uploaded to Partner Center, and a PNG of a window is not source).

.NOTES
    Stops any running WinRemap and starts its own, so do not run this while
    depending on a remap. The original is not restarted; do that yourself.
#>
[CmdletBinding()]
param(
    [ValidateSet('en', 'ja')]
    [string]$Lang = 'en',
    [string]$Config,
    [ValidateSet('release', 'debug')]
    [string]$Configuration = 'release',
    [string]$OutDir,
    # Pauses before the log shot so a person can press real keys. Synthetic
    # input cannot stand in: WinRemap passes injected events through untouched
    # by design (AGENTS.md invariant 1), so nothing typed by a script is ever
    # remapped, and a remap is the only thing worth showing in a log.
    [switch]$LogInteractive,
    [int]$InteractiveSeconds = 30
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, UIAutomationClient, UIAutomationTypes

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Resolve-Path (Join-Path $here '..\..')
if (-not $OutDir) { $OutDir = Join-Path $here 'screenshots' }
if (-not $Config) {
    # A config written for these images. examples/ holds configs meant to be
    # copied; a screenshot has a different job — show several keymaps, an
    # app-specific one beside a global one, and all three kinds of rule, in
    # one frame. The owner's own config would also put his name in a
    # published image for no gain.
    $Config = "packaging\msix\screenshot-demo.$Lang.toml"
}

$CANVAS_W = 1920
$CANVAS_H = 1080

$capSource = @'
using System;
using System.Runtime.InteropServices;
public class Cap {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint f);
  // Z-order, not focus. Windows grants this to anyone, unlike the
  // foreground, so it is the one way to guarantee nothing is drawn over the
  // window about to be photographed.
  public static void Raise(IntPtr h, bool on) {
    IntPtr TOPMOST = new IntPtr(-1), NOTOPMOST = new IntPtr(-2);
    SetWindowPos(h, on ? TOPMOST : NOTOPMOST, 0, 0, 0, 0, 0x0013); // NOSIZE|NOMOVE|NOACTIVATE
  }
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int size);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  public const uint RIGHTDOWN = 0x0008, RIGHTUP = 0x0010;
  public static void RightClick(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(300);
    mouse_event(RIGHTDOWN, 0, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(80);
    mouse_event(RIGHTUP, 0, 0, 0, IntPtr.Zero);
  }
  public static void Key(byte vk) {
    keybd_event(vk, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(60);
    keybd_event(vk, 0, 2, IntPtr.Zero);
    System.Threading.Thread.Sleep(300);
  }
  // Which application is actually in front. SetForegroundWindow reports
  // success while doing nothing when the caller has no foreground rights, so
  // the only trustworthy answer is to ask afterwards.
  public static string ForegroundExe() {
    uint pid;
    GetWindowThreadProcessId(GetForegroundWindow(), out pid);
    try { return System.Diagnostics.Process.GetProcessById((int)pid).ProcessName; }
    catch { return "?"; }
  }
  // The frame Windows actually draws. GetWindowRect includes the invisible
  // resize border, which would put a band of desktop into the image.
  public static RECT FrameOf(IntPtr h) {
    RECT r;
    // 9 = DWMWA_EXTENDED_FRAME_BOUNDS
    if (DwmGetWindowAttribute(h, 9, out r, Marshal.SizeOf(typeof(RECT))) == 0) return r;
    GetWindowRect(h, out r);
    return r;
  }
}
'@

# .NET cannot replace a type once it is loaded, so a second run in the same
# PowerShell session keeps whatever this script defined the first time. Left
# alone that fails later and further from the cause — an older Cap without
# ForegroundExe would take the foreground check out of the run, which is
# exactly the check that exists because guessing there wasted two attempts.
$capType = 'Cap' -as [type]
if (-not $capType) {
    Add-Type -TypeDefinition $capSource
}
elseif (-not $capType.GetMethod('Raise')) {
    throw @'
An older copy of this script's helper type is loaded in this PowerShell
session, and .NET cannot replace it. Open a new PowerShell window and run
the script again.
'@
}

[void][Cap]::SetProcessDPIAware()

$root = [System.Windows.Automation.AutomationElement]::RootElement
$desc = [System.Windows.Automation.TreeScope]::Descendants
$kids = [System.Windows.Automation.TreeScope]::Children
$any = [System.Windows.Automation.Condition]::TrueCondition
$textType = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Text)

function Say([string]$m) { Write-Host $m -ForegroundColor Cyan }

# --- window plumbing -------------------------------------------------------

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

function Wait-Window([string]$TitleLike, [int]$Seconds = 12) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        $w = Get-WindowLike $TitleLike
        if ($w) { return $w }
        Start-Sleep -Milliseconds 400
    }
    return $null
}

function Find-Named($parent, [string]$name) {
    if (-not $parent) { return $null }
    foreach ($e in $parent.FindAll($desc, $any)) { if ($e.Current.Name -eq $name) { return $e } }
    return $null
}

function Click-Element($e) {
    $r = $e.Current.BoundingRectangle
    [void][Cap]::SetCursorPos([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
    Start-Sleep -Milliseconds 250
    [Cap]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)   # LEFTDOWN
    Start-Sleep -Milliseconds 80
    [Cap]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)   # LEFTUP
    Start-Sleep -Seconds 2
}

# InvokePattern where the element offers it, a real click where it does not —
# egui's tree items expose no Invoke, and the keymap nodes are tree items.
function Invoke-Named($parent, [string]$name) {
    $e = Find-Named $parent $name
    if (-not $e) { Say "  '$name' not found"; return $false }
    try {
        $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Seconds 2
    }
    catch { Click-Element $e }
    return $true
}

# --- tray ------------------------------------------------------------------

# Windows 11 files a new notification-area icon into the overflow flyout, which
# cannot be driven reliably from automation. The per-icon IsPromoted flag puts
# it on the taskbar instead, where it can be right-clicked. Same approach as
# tests/ui/guest/promote-tray-icon.ps1.
function Set-IconPromoted([string]$ExePath) {
    $root = 'HKCU:\Control Panel\NotifyIconSettings'
    if (-not (Test-Path $root)) { return 0 }
    $n = 0
    foreach ($k in Get-ChildItem $root) {
        $p = Get-ItemProperty $k.PSPath
        if ($p.PSObject.Properties.Name -contains 'ExecutablePath' -and $p.ExecutablePath -eq $ExePath) {
            Set-ItemProperty $k.PSPath -Name IsPromoted -Value 1 -Type DWord
            $n++
        }
    }
    return $n
}

function Find-TrayIcon {
    $btn = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Button)
    foreach ($e in $root.FindAll($desc, $btn)) {
        if ($e.Current.Name -notlike '*WinRemap*') { continue }
        # A taskbar button for an open window is also a Button named after
        # WinRemap; only the icon lacks an "Appid:" automation id.
        if ("$($e.Current.AutomationId)" -like 'Appid:*') { continue }
        return $e
    }
    return $null
}

function Open-TrayMenu {
    foreach ($attempt in 1..4) {
        [Cap]::Key(0x1B)   # Esc: a menu left open swallows the next click
        Start-Sleep -Milliseconds 700
        $icon = Find-TrayIcon
        if (-not $icon) { Say "  tray icon not found (attempt $attempt)"; Start-Sleep -Seconds 2; continue }
        $r = $icon.Current.BoundingRectangle
        [Cap]::RightClick([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
        for ($i = 0; $i -lt 12; $i++) {
            $h = [Cap]::FindWindow('#32768', $null)
            if ($h -ne [IntPtr]::Zero) { return $h }
            Start-Sleep -Milliseconds 400
        }
        Say "  tray menu did not open (attempt $attempt)"
    }
    return [IntPtr]::Zero
}

# The popup's items are not in the UIA tree, so selection is by keyboard.
#
# Menu order (src/tray.rs): title (disabled), --, Enabled, --, Settings,
# Reload config, Show log, --, Quit.
#
# Counted from the bottom, not the top. Arrow keys skip separators but may or
# may not land on a disabled item, and the only disabled item is the title at
# the very top — so counting up from Quit is exact while counting down from
# the top is a guess. It also never touches Enabled, which the first version
# of this script did: it probed downwards, switched remapping off on the way
# past, and left it off. Every later keystroke then passed through unremapped
# and the log shot came out empty, with nothing on screen to say why.
$MENU_UP = @{ quit = 1; log = 2; reload = 3; settings = 4 }

function Invoke-MenuFromBottom([int]$Up) {
    if ($Up -lt 2) { throw "refusing to select menu item $Up from the bottom (that is Quit)" }
    $h = Open-TrayMenu
    if ($h -eq [IntPtr]::Zero) { return $false }
    for ($i = 0; $i -lt $Up; $i++) { [Cap]::Key(0x26) }   # VK_UP
    [Cap]::Key(0x0D)                                      # VK_RETURN
    Start-Sleep -Seconds 2
    return $true
}

# --- capture ---------------------------------------------------------------

function Save-WindowShot($window, [string]$Name, [string]$Caption) {
    if (-not $window) { Say "  no window for $Name"; return }
    $h = [IntPtr]$window.Current.NativeWindowHandle
    # The image is read off the screen, so anything drawn over the window
    # lands in it — a Notepad window did exactly that once. Asking for the
    # foreground is not enough, since Windows may refuse; pinning the z-order
    # always works and needs no foreground rights.
    [Cap]::Raise($h, $true)
    [void][Cap]::SetForegroundWindow($h)
    Start-Sleep -Milliseconds 900
    $r = [Cap]::FrameOf($h)
    Save-RectShot $r $Name $Caption
    [Cap]::Raise($h, $false)
}

function Save-RectShot($r, [string]$Name, [string]$Caption) {
    $w = $r.Right - $r.Left
    $h = $r.Bottom - $r.Top
    if ($w -le 0 -or $h -le 0) { Say "  empty rect for $Name"; return }

    $shot = New-Object System.Drawing.Bitmap($w, $h)
    $g = [System.Drawing.Graphics]::FromImage($shot)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
    $g.Dispose()

    $canvas = New-Object System.Drawing.Bitmap($CANVAS_W, $CANVAS_H)
    $cg = [System.Drawing.Graphics]::FromImage($canvas)
    $cg.Clear($script:BackColor)
    $cg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic

    # Fit to the canvas, and allow a modest enlargement for something as small
    # as the tray menu — left at 1:1 it is a postage stamp in a field of grey,
    # which reads as a mistake rather than as a menu. Capped at 2x: beyond
    # that the softening is worse than the empty space.
    $scale = [Math]::Min(2.0, [Math]::Min(($CANVAS_W - 120) / $w, ($CANVAS_H - 120) / $h))
    $dw = [int]($w * $scale)
    $dh = [int]($h * $scale)
    $cg.DrawImage($shot, [int](($CANVAS_W - $dw) / 2), [int](($CANVAS_H - $dh) / 2), $dw, $dh)
    $cg.Dispose()

    $path = Join-Path $OutDir "$Name.png"
    $canvas.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $canvas.Dispose(); $shot.Dispose()
    Say "  saved $Name.png  (window ${w}x${h}, scale $([Math]::Round($scale,2)))  $Caption"
}

# --- run -------------------------------------------------------------------

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# Match the canvas to the system theme so the window does not sit on a colour
# that fights it.
$light = 1
try {
    $light = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -Name AppsUseLightTheme).AppsUseLightTheme
}
catch { }
$script:BackColor = if ($light -eq 1) {
    [System.Drawing.Color]::FromArgb(0xEC, 0xEF, 0xF3)
}
else {
    [System.Drawing.Color]::FromArgb(0x1B, 0x1E, 0x22)
}

$exe = Join-Path $repo "target\$Configuration\winremap.exe"
if (-not (Test-Path $exe)) { throw "missing $exe (cargo build --$Configuration)" }

$titles = if ($Lang -eq 'ja') {
    @{ settings = 'WinRemap*設定*'; log = 'WinRemap*ログ*'; edit = '編集'; keymap = 'ターミナル' }
}
else {
    @{ settings = 'WinRemap*settings*'; log = 'WinRemap*log*'; edit = 'Edit'; keymap = 'terminal' }
}

Say "language $Lang, config $Config"

# Notepad has to be open before WinRemap starts, not after. Opening it later
# means going through the Start menu, and those foreground changes —
# searchhost, startmenuexperiencehost, explorer — are written into the very
# log this run is about to photograph.
if ($LogInteractive) {
    $told = $false
    while (-not (Get-Process -Name Notepad -ErrorAction SilentlyContinue |
                Where-Object { $_.MainWindowHandle -ne 0 })) {
        if (-not $told) {
            Write-Host ''
            Write-Host '  Open Notepad before this goes any further, and type a' -ForegroundColor Yellow
            Write-Host '  sentence in it so there is something to delete later.' -ForegroundColor Yellow
            Write-Host '  Waiting...' -ForegroundColor Yellow
            $told = $true
        }
        Start-Sleep -Milliseconds 500
    }
    Say '  notepad is open'
}

Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2

$promoted = Set-IconPromoted $exe
Say "promoted $promoted tray icon entr$(if ($promoted -eq 1) { 'y' } else { 'ies' })"

Start-Process $exe -ArgumentList '--config', $Config, '--lang', $Lang -WorkingDirectory $repo
Start-Sleep -Seconds 6

# 1. Tray menu, before anything else covers the taskbar corner.
$h = Open-TrayMenu
if ($h -ne [IntPtr]::Zero) {
    $r = [Cap]::FrameOf($h)
    Save-RectShot $r "$Lang-04-tray-menu" 'tray menu'
    [Cap]::Key(0x1B)
}
else { Say '  skipped tray menu shot' }

# 2. Settings window, showing a keymap's rules.
#
# The window opens on General, which for a keyboard remapper shows a macro
# delay and an IME toggle — true, and no use at all as the first image a
# customer sees. Selecting a keymap puts the rule table on screen, which is
# the thing the product does.
if (-not (Invoke-MenuFromBottom $MENU_UP.settings)) { throw 'could not open the tray menu' }
$settings = Wait-Window $titles.settings
if (-not $settings) { throw 'the settings window did not open — check the tray menu order in src/tray.rs' }
if (-not (Invoke-Named $settings $titles.keymap)) { Say '  keymap node not selected; General page will be shown' }
Save-WindowShot (Get-WindowLike $titles.settings) "$Lang-01-settings" 'settings window, keymap rules'

# 3. Settings window, editing.
if (Invoke-Named (Get-WindowLike $titles.settings) $titles.edit) {
    Start-Sleep -Seconds 1
    Save-WindowShot (Get-WindowLike $titles.settings) "$Lang-02-settings-edit" 'settings window, edit mode'
}
else { Say '  skipped edit-mode shot' }

# 4. Log window.
if (Invoke-MenuFromBottom $MENU_UP.log) {
    $log = Wait-Window $titles.log

    # An empty log is not a picture of a log, and there is no way around a
    # person for this one: injected keys are passed through untouched by
    # design, so the only thing that reaches this window is somebody typing.
    if ($LogInteractive) {
        # Notepad is opened by that person before the script starts (checked
        # up front). Launching it here never worked — a script cannot hand its
        # own window the foreground — and opening it from the Start menu
        # mid-run wrote searchhost and explorer lines into the very log about
        # to be photographed.
        #
        # Which window is in front decides which keymap applies, so the
        # countdown must not start until Notepad really is in front. Earlier
        # versions gave up after a while and carried on; both times that
        # produced a log of keys correctly passed through by the global
        # keymap — a picture of the product doing nothing, for a reason
        # nothing on screen explained. There is no timeout here on purpose.
        $asked = $false
        while ([Cap]::ForegroundExe() -notlike 'Notepad*') {
            if (-not $asked) {
                Write-Host ''
                Write-Host '  Click your Notepad window now.' -ForegroundColor Red
                Write-Host '  The countdown starts as soon as it is in front.' -ForegroundColor Red
                $asked = $true
            }
            Start-Sleep -Milliseconds 500
        }
        Say "  foreground is $([Cap]::ForegroundExe())"
        Write-Host ''
        Write-Host '  ------------------------------------------------------------' -ForegroundColor Yellow
        Write-Host '  Notepad is in front. Type there — do NOT click any other' -ForegroundColor Yellow
        Write-Host '  window, or the rules of a different keymap apply. Remapped' -ForegroundColor Yellow
        Write-Host '  in Notepad by the demo config:' -ForegroundColor Yellow
        Write-Host '     Ctrl+H     -> Backspace   (one line per press)' -ForegroundColor Yellow
        Write-Host '     Ctrl+T     -> a macro of three keystrokes' -ForegroundColor Yellow
        Write-Host '     Alt+X then U -> Ctrl+Z    (a two-stroke sequence)' -ForegroundColor Yellow
        Write-Host '  Type a sentence first so there is something to delete.' -ForegroundColor Yellow
        Write-Host "  Capturing in $InteractiveSeconds seconds." -ForegroundColor Yellow
        Write-Host '  ------------------------------------------------------------' -ForegroundColor Yellow
        for ($s = $InteractiveSeconds; $s -gt 0; $s--) {
            $fg = [Cap]::ForegroundExe()
            $mark = if ($fg -like 'Notepad*') { '' } else { "  <-- $fg is in front, not Notepad!" }
            Write-Host -NoNewline "`r  $s $mark          "
            Start-Sleep -Seconds 1
        }
        Write-Host "`r                                                    "
        # Notepad is left running. Windows 11 runs it as a single process
        # shared by every window, so force-killing it would take the user's
        # own tabs with it.
        Start-Sleep -Seconds 1
    }

    $logWindow = Get-WindowLike $titles.log
    Save-WindowShot $logWindow "$Lang-03-log" 'log window'

    # Say whether the image is worth keeping, instead of leaving it to be
    # discovered later: a log of keys that were all passed through shows the
    # product doing nothing.
    if ($LogInteractive -and $logWindow) {
        $lines = @()
        foreach ($t in $logWindow.FindAll($desc, $textType)) { if ($t.Current.Name) { $lines += $t.Current.Name } }
        $marker = if ($Lang -eq 'ja') { '*置換*' } else { '*remapped*' }
        $hits = @($lines | Where-Object { $_ -like $marker }).Count
        if ($hits -gt 0) { Say "  the log shows $hits remapped key(s)" }
        else {
            Write-Host '  WARNING: no remapped keys in the log.' -ForegroundColor Red
            Write-Host '  The keys went to a window other than Notepad, where a' -ForegroundColor Red
            Write-Host '  different keymap applies. Re-run and type in Notepad only.' -ForegroundColor Red
        }
    }
}
else { Say '  skipped log shot' }

Say ''
Say "done. $OutDir"
Get-ChildItem $OutDir -Filter "$Lang-*.png" | ForEach-Object { "  $($_.Name)  $([Math]::Round($_.Length/1KB)) KB" }
