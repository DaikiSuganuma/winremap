# Guest side, one shot. The measurement gate of docs/v0.5/notes/20260727_winapp-cli-migration.md section 3.
#
# Everything is in one script on purpose: a tray menu is modal and is released
# when the process that opened it exits, so a probe split across run-in-vm
# calls would measure the split, not the tool.
#
# P1 settings window contents      P5 wait-for instead of Start-Sleep
# P2 log window lines              P6 tray route, promoted (a) and overflow (b)
# P3 selector stability            P7 wall clock
# P4 invoke, ten times per button

param([int]$Reps = 10, [string]$Out = "C:\Test\winapp-probe.txt")

$ErrorActionPreference = "Continue"
$env:Path = $env:Path + ';' + $env:LOCALAPPDATA + '\Microsoft\WindowsApps'

$lines = New-Object System.Collections.Generic.List[string]
$checks = [ordered]@{}
$total = [System.Diagnostics.Stopwatch]::StartNew()
function Say([string]$s) { $lines.Add($s) }
function Check([string]$name, [bool]$ok, [string]$detail) {
    $checks[$name] = $ok
    Say ("CHECK {0,-26} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}
function Flush { $lines | Set-Content $Out -Encoding UTF8 }
trap { Say "EXCEPTION: $_"; Say $_.ScriptStackTrace; Flush; exit 1 }

# --- winapp helpers -------------------------------------------------------
# One [string[]] parameter, not ValueFromRemainingArguments: an array handed to
# a remaining-arguments parameter collapses into a single string, and winapp
# then sees one long argument and prints its usage. That is what the migration
# note means by "pass winapp's arguments as an array".
function W {
    param([string[]]$A)
    $text = (& winapp @A 2>&1 | Out-String)
    return [pscustomobject]@{ Code = $LASTEXITCODE; Text = $text.TrimEnd() }
}
# ConvertFrom-Json hands a JSON array to the pipeline as ONE object in
# PowerShell 5.1, so a function that returns it straight through yields a
# single item whose properties are arrays. `$w.className -eq "Shell_TrayWnd"`
# then matches "the whole list" and the caller ends up holding every window at
# once. @() before returning is what flattens it back to one object per window.
function WJson {
    param([string[]]$A)
    $text = (& winapp @A 2>$null | Out-String)
    if ($LASTEXITCODE -ne 0 -or -not $text.Trim()) { return @() }
    # Assigned to a variable first on purpose: `return @(pipeline)` keeps the
    # array as one item, because ConvertFrom-Json emits it as one item and @()
    # only flattens the wrapper it just created. Through a variable the array
    # enumerates, and the caller gets one object per window.
    try { $obj = $text | ConvertFrom-Json } catch { return @() }
    return @($obj)
}
# inspect --json is { windows: [ { elements: [ { ..., children: [...] } ] } ] }.
# Always read the JSON, never the text output: the text comes back as mojibake
# through PowerShell 5.1 (UTF-8 decoded as CP932), while \uXXXX in the JSON
# survives ConvertFrom-Json intact - and this guest's UI is Japanese.
function Flatten($node, [System.Collections.Generic.List[object]]$acc) {
    if ($null -eq $node) { return }
    foreach ($n in @($node)) {
        if ($null -eq $n) { continue }
        $acc.Add($n)
        if ($n.PSObject.Properties.Name -contains "children") { Flatten $n.children $acc }
    }
}
function Elements([string[]]$Target) {
    $json = WJson (@("ui", "inspect", "--json", "-d", "20") + $Target)
    $acc = New-Object System.Collections.Generic.List[object]
    foreach ($w in @($json)) {
        if ($w.PSObject.Properties.Name -contains "windows") {
            foreach ($win in @($w.windows)) { Flatten $win.elements $acc }
        }
    }
    return $acc
}
function NameOf($e) {
    if ($e.PSObject.Properties.Name -contains "name" -and $e.name) { return [string]$e.name }
    return ""
}
function HasName($els, [string]$name) {
    foreach ($e in $els) { if ((NameOf $e) -eq $name) { return $true } }
    return $false
}

# --- plain Win32, for the parts that are not under measurement ------------
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class N {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string n);
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public const uint WM_CLOSE = 0x0010;
}
'@

function Start-WinRemap {
    Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    Start-Process C:\Test\winremap.exe -ArgumentList '--config', 'C:\Test\minimal.toml', '--lang', 'en'
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
        if (Get-Process winremap -ErrorAction SilentlyContinue) { return $true }
        Start-Sleep -Milliseconds 300
    }
    return $false
}

function Get-Window([string]$TitleLike) {
    $wins = WJson @("ui", "list-windows", "--app", "winremap", "--json")
    if (-not $wins) { return $null }
    foreach ($w in @($wins)) {
        if ($w.PSObject.Properties.Name -contains "title" -and $w.title -like $TitleLike) { return $w }
    }
    return $null
}

function Close-Window([string]$TitleLike) {
    $w = Get-Window $TitleLike
    if (-not $w) { return }
    [N]::PostMessage([IntPtr]$w.hwnd, [N]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Seconds 2
}

# =========================================================================
# P6(b) the overflow route, winapp only
# =========================================================================
Say ("winapp " + (W "--version").Text + "   reps=$Reps")
Say ""
Say "=== P6(b) overflow route ==="

if (-not (Start-WinRemap)) { Check "app-starts" $false "winremap did not start"; Flush; exit 1 }
Start-Sleep -Seconds 5

$tray = $null
foreach ($w in @(WJson @("ui", "list-windows", "--app", "explorer", "--json"))) {
    if ($w.className -eq "Shell_TrayWnd" -and -not $tray) { $tray = $w }
}
Say ("Shell_TrayWnd hwnd=" + $(if ($tray) { $tray.hwnd } else { "NOT FOUND" }))

# Find the chevron by what it does, not by what it is called: this guest runs
# Japanese Windows, and matching its name would tie the probe to the display
# language. Every notification-area button is tried in taskbar order until one
# of them makes an overflow window appear; that one is the chevron.
$trayButtons = @()
if ($tray) {
    $taskbar = Elements @("-w", "$($tray.hwnd)")
    Say ("taskbar elements: " + $taskbar.Count)
    foreach ($e in $taskbar) { Say ("  " + $e.type + " '" + (NameOf $e) + "' selector=" + $e.selector) }
    $trayButtons = @($taskbar | Where-Object { "$($_.selector)" -like "btn-systemtrayicon-*" } | Sort-Object x)
    Say ("notification-area buttons: " + $trayButtons.Count)
}
$chevron = $null

function Get-OverflowWindow {
    foreach ($w in @(WJson @("ui", "list-windows", "--app", "explorer", "--json"))) {
        if ($w.className -match "Overflow") { return $w }
    }
    return $null
}

function Open-TrayMenuViaOverflow {
    if (-not $tray) { return $null }
    $overflow = $null
    # invoke on the chevron TOGGLES the overflow, so a run that left it open
    # closes it here instead of opening it - that is the every-other-rep
    # failure the verification report warned about. Start from closed.
    if (Get-OverflowWindow) {
        if ($chevron) { W @("ui", "invoke", $chevron.selector, "-w", "$($tray.hwnd)") | Out-Null }
        Start-Sleep -Seconds 2
    }
    # Once the chevron is known, press only that one.
    $candidates = if ($chevron) { @($chevron) } else { $trayButtons }
    foreach ($c in $candidates) {
        $inv = W @("ui", "invoke", $c.selector, "-w", "$($tray.hwnd)")
        if ($inv.Code -ne 0) { Say ("  invoke " + $c.selector + " exit=" + $inv.Code); continue }
        Start-Sleep -Seconds 2
        $overflow = Get-OverflowWindow
        if ($overflow) {
            if (-not $chevron) { $script:chevron = $c; Say ("chevron is " + $c.selector + " '" + (NameOf $c) + "'") }
            break
        }
    }
    if (-not $overflow) { Say "  overflow window NOT FOUND"; return $null }
    $icon = $null
    foreach ($e in (Elements @("-w", "$($overflow.hwnd)"))) {
        if ((NameOf $e) -like "*WinRemap*") { $icon = $e }
    }
    if (-not $icon) { Say "  WinRemap icon NOT FOUND in the overflow"; return $null }
    $click = W @("ui", "click", $icon.selector, "-w", "$($overflow.hwnd)", "--right")
    if ($click.Code -ne 0) { Say ("  right-click exit=" + $click.Code + " " + $click.Text); return $null }
    Start-Sleep -Seconds 2
    if ([N]::FindWindow("#32768", $null) -eq [IntPtr]::Zero) { Say "  no #32768 after right-click"; return $null }
    $menu = $null
    foreach ($w in @(WJson @("ui", "list-windows", "--app", "winremap", "--json"))) {
        if ($w.className -eq "#32768") { $menu = $w }
    }
    if (-not $menu) { Say "  menu window not in list-windows"; return $null }
    return $menu
}

$trayOk = 0
$menuIds = ""
foreach ($i in 1..$Reps) {
    Close-Window "*ettings*"
    $menu = Open-TrayMenuViaOverflow
    if (-not $menu) { Say ("  rep ${i}: menu did not open"); continue }
    if ($i -eq 1) {
        $ins = W @("ui", "inspect", "-w", "$($menu.hwnd)", "-i")
        Say "menu contents:"; Say $ins.Text
        $menuIds = $ins.Text
    }
    # 1003 = Settings. The report says menu items carry the app's command ids.
    $inv = W @("ui", "invoke", "1003", "-w", "$($menu.hwnd)")
    Start-Sleep -Seconds 3
    $settings = Get-Window "*ettings*"
    if ($inv.Code -eq 0 -and $settings) { $trayOk++ }
    else { Say ("  rep ${i}: invoke exit=" + $inv.Code + " settings=" + [bool]$settings) }
}
Check "P6b-overflow-route" ($trayOk -eq $Reps) "$trayOk of $Reps : chevron -> right-click -> invoke 1003 opened the settings window"
Check "P6b-menu-ids" ($menuIds -match "1003") "menu items are addressable by command id"

# =========================================================================
# P1 / P2  what the windows expose to winapp
# =========================================================================
Say ""
Say "=== P1/P2 window contents ==="
if (-not (Get-Window "*ettings*")) {
    $menu = Open-TrayMenuViaOverflow
    if ($menu) { W @("ui", "invoke", "1003", "-w", "$($menu.hwnd)") | Out-Null; Start-Sleep -Seconds 3 }
}
$settings = Get-Window "*ettings*"
$sEls = if ($settings) { Elements @("-w", "$($settings.hwnd)") } else { @() }
Say ("settings window: " + $(if ($settings) { "hwnd=" + $settings.hwnd + " title='" + $settings.title + "'" } else { "NOT OPEN" }))
Say ("elements: " + $sEls.Count)
foreach ($e in $sEls) { Say ("  " + $e.type + " '" + (NameOf $e) + "' selector=" + $e.selector) }
Check "P1-settings-elements" ($sEls.Count -ge 40) "$($sEls.Count) elements (plain UIA sees 43)"
Check "P1-settings-names" ((HasName $sEls "Edit") -and (HasName $sEls "General") -and (HasName $sEls "Keymaps") -and (HasName $sEls "notepad")) `
    "Edit / General / Keymaps / notepad are all addressable by name"

$menu = Open-TrayMenuViaOverflow
if ($menu) { W @("ui", "invoke", "1004", "-w", "$($menu.hwnd)") | Out-Null; Start-Sleep -Seconds 3 }
$logw = Get-Window "*og*"
$lEls = if ($logw) { Elements @("-w", "$($logw.hwnd)") } else { @() }
Say ("log window: " + $(if ($logw) { "hwnd=" + $logw.hwnd + " title='" + $logw.title + "'" } else { "NOT OPEN" }))
foreach ($e in $lEls) { Say ("  " + $e.type + " '" + (NameOf $e) + "' selector=" + $e.selector) }
$logTexts = @($lEls | Where-Object { $_.type -match "(?i)text" })
Check "P2-log-lines" ($logTexts.Count -gt 0) "$($logTexts.Count) text elements in the log window (plain UIA sees 7 in total)"

# =========================================================================
# P4 / P5  invoke and wait-for, ten times each
# =========================================================================
Say ""
Say "=== P4/P5 invoke + wait-for ==="
$editOk = 0; $revertOk = 0; $waitOk = 0
$settings = Get-Window "*ettings*"
if ($settings) {
    $w = @("-w", "$($settings.hwnd)")
    # Edit belongs to a keymap page, so select the keymap first - the same
    # order dump-uia.ps1 uses. The tree item carries no Invoke pattern, hence
    # click rather than invoke (scenario 02 says the same).
    # invoke first: it tries SelectionItemPattern too, which a tree item may
    # have even when it has no Invoke. Fall back to a click by selector, taken
    # from search rather than typed by hand.
    $sel = W (@("ui", "invoke", "notepad") + $w)
    Say ("select keymap 'notepad' via invoke: exit=" + $sel.Code + " " + $sel.Text)
    if ($sel.Code -ne 0) {
        $hit = WJson (@("ui", "search", "notepad", "--json") + $w)
        $slug = $null
        foreach ($h in @($hit)) {
            if ($h.PSObject.Properties.Name -contains "matches") {
                foreach ($m in @($h.matches)) { if ((NameOf $m) -eq "notepad" -and -not $slug) { $slug = $m.selector } }
            }
        }
        Say ("  search says selector=" + $slug)
        if ($slug) {
            $sel = W (@("ui", "click", $slug) + $w)
            Say ("  click " + $slug + ": exit=" + $sel.Code + " " + $sel.Text)
        }
    }
    Start-Sleep -Seconds 2
    $wait0 = W (@("ui", "wait-for", "Edit", "--timeout", "5000") + $w)
    Say ("Edit present after selecting: exit=" + $wait0.Code)
    $afterSelect = Elements $w
    Say ("elements after selecting: " + $afterSelect.Count + " (43 -> 59 through plain UIA)")
    foreach ($i in 1..$Reps) {
        $inv = W (@("ui", "invoke", "Edit") + $w)
        # No Start-Sleep here on purpose: this is what P5 measures.
        #
        # Waiting on "Revert", not on "Save": the word Save also appears in the
        # status line ("the file is untouched until you press Save"), and
        # wait-for reports an ambiguous name as NOT FOUND rather than as
        # ambiguous - which is how a working assertion looked like a failing
        # one for two runs of this probe. Names used as selectors have to be
        # unique in the window, or resolved to a slug first (below).
        $wait = W (@("ui", "wait-for", "Revert", "--timeout", "5000") + $w)
        if ($inv.Code -eq 0 -and $wait.Code -eq 0) { $editOk++; $waitOk++ }
        else {
            Say ("  rep ${i}: invoke Edit=" + $inv.Code + " wait Save=" + $wait.Code)
            if ($i -eq 1) {
                Say ("    invoke said: " + $inv.Text)
                Say ("    the window now holds:")
                foreach ($e in (Elements $w)) { Say ("      " + $e.type + " '" + (NameOf $e) + "'") }
            }
        }
        $rev = W (@("ui", "invoke", "Revert") + $w)
        $wait2 = W (@("ui", "wait-for", "Edit", "--timeout", "5000") + $w)
        if ($rev.Code -eq 0 -and $wait2.Code -eq 0) { $revertOk++ }
        else { Say ("  rep ${i}: invoke Revert=" + $rev.Code + " wait Edit=" + $wait2.Code) }
    }
}
Check "P4-invoke-edit" ($editOk -eq $Reps) "$editOk of $Reps (terminator 2/6, plain UIA 4/4)"

# The ambiguous-name case, measured rather than assumed: search resolves the
# button to a slug, and wait-for takes the slug.
$slugOk = 0
if ($settings) {
    $w = @("-w", "$($settings.hwnd)")
    foreach ($i in 1..3) {
        W (@("ui", "invoke", "Edit") + $w) | Out-Null
        $hit = WJson (@("ui", "search", "Save", "--json") + $w)
        $slug = $null
        foreach ($h in @($hit)) {
            foreach ($m in @($h.matches)) { if ($m.type -eq "Button" -and -not $slug) { $slug = $m.selector } }
        }
        $byName = W (@("ui", "wait-for", "Save", "--timeout", "2000") + $w)
        $bySlug = if ($slug) { W (@("ui", "wait-for", $slug, "--timeout", "2000") + $w) } else { $null }
        Say ("  ambiguous 'Save': by name exit=" + $byName.Code + "  by slug ($slug) exit=" + $(if ($bySlug) { $bySlug.Code } else { "n/a" }))
        if ($bySlug -and $bySlug.Code -eq 0 -and $byName.Code -ne 0) { $slugOk++ }
        W (@("ui", "invoke", "Revert") + $w) | Out-Null
        Start-Sleep -Seconds 1
    }
}
Check "P5-ambiguous-name" ($slugOk -eq 3) `
    "$slugOk of 3: a name matching two elements fails as 'not found', the slug from search works"
Check "P4-invoke-revert" ($revertOk -eq $Reps) "$revertOk of $Reps"
Check "P5-wait-for" ($waitOk -eq $Reps) "$waitOk of $Reps transitions caught with no fixed sleep"

# The negative case: an assertion that cannot fail proves nothing.
$neg = W @("ui", "wait-for", "NoSuchElementInThisWindow", "--app", "winremap", "--timeout", "2000")
Check "P5-negative-case" ($neg.Code -ne 0) "a missing element exits $($neg.Code), not 0"
$neg2 = W @("ui", "invoke", "NoSuchButtonHere", "--app", "winremap")
Check "P4-negative-case" ($neg2.Code -ne 0) "invoking a missing element exits $($neg2.Code), not 0"

# =========================================================================
# P3  do the same selectors resolve after a restart?
# =========================================================================
Say ""
Say "=== P3 selector stability across restarts ==="
$stable = 0
foreach ($i in 1..3) {
    Start-WinRemap | Out-Null
    Start-Sleep -Seconds 5
    $menu = Open-TrayMenuViaOverflow
    if ($menu) { W @("ui", "invoke", "1003", "-w", "$($menu.hwnd)") | Out-Null; Start-Sleep -Seconds 3 }
    $s = Get-Window "*ettings*"
    if ($s) {
        $els = Elements @("-w", "$($s.hwnd)")
        if ((HasName $els "Edit") -and (HasName $els "Keymaps")) { $stable++ }
        else { Say ("  restart ${i}: names did not resolve") }
    }
    else { Say ("  restart ${i}: settings window did not open") }
}
Check "P3-selector-stability" ($stable -eq 3) "$stable of 3 restarts resolved Edit and Keymaps by name"

# =========================================================================
# P6(a) the promoted, visible tray icon
# =========================================================================
Say ""
Say "=== P6(a) promoted icon on the taskbar ==="
$root = "HKCU:\Control Panel\NotifyIconSettings"
$promoted = 0
if (Test-Path $root) {
    foreach ($k in Get-ChildItem $root) {
        $p = Get-ItemProperty $k.PSPath
        if ($p.PSObject.Properties.Name -contains "ExecutablePath" -and $p.ExecutablePath -like "*winremap.exe") {
            Set-ItemProperty $k.PSPath -Name IsPromoted -Value 1 -Type DWord; $promoted++
        }
    }
}
Say ("promoted registry entries: " + $promoted)
Start-WinRemap | Out-Null
Start-Sleep -Seconds 6
$icon = $null
if ($tray) {
    foreach ($e in (Elements @("-w", "$($tray.hwnd)"))) {
        if ((NameOf $e) -like "*WinRemap*") { $icon = $e; Say ("visible tray icon: '" + (NameOf $e) + "' selector=" + $e.selector) }
    }
}
$visibleClick = if ($icon) { W @("ui", "click", $icon.selector, "-w", "$($tray.hwnd)", "--right") } else { $null }
if ($visibleClick) { Say ("click --right exit=" + $visibleClick.Code); Say $visibleClick.Text }
$menuAfter = ([N]::FindWindow("#32768", $null) -ne [IntPtr]::Zero)
Check "P6a-visible-icon-seen" ([bool]$icon) "the promoted icon is in the tree"
Check "P6a-visible-right-click" ($menuAfter) "right-clicking the promoted icon opened the menu"

# =========================================================================
Say ""
Say ("=== P7 wall clock: {0:mm\:ss} for the whole probe ===" -f $total.Elapsed)
Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
