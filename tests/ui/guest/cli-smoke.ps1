# Runs inside the guest, in session 1. Checks the parts of WinRemap that are
# decided before any window exists: the command line, where output goes, and
# what a launch with no terminal does with an error.
#
# There is no AI in this loop, for the same reason as dump-uia.ps1: these are
# the checks an agent would be worst at, because "nothing was printed" and
# "the agent did not look" are indistinguishable from the outside.
#
# Covers (docs/v0.5/03_acceptance-checklist.md section 5): the v0.1 startup
# smoke items and v0.2 A-1, A-4, A-5, A-6, A-8, A-9, B0-11.
#
# Host side: .\run-vm-ui-test.ps1 -Scenario cli-smoke   (or as part of `all`)

$ErrorActionPreference = "Continue"
$exe = "C:\Test\winremap.exe"
$outPath = "C:\Test\cli-smoke.txt"

$lines = New-Object System.Collections.Generic.List[string]
$checks = [ordered]@{}
function Say([string]$s) { $lines.Add($s) }
# $Ok is deliberately untyped. `-match` against an array returns the matching
# elements rather than a boolean, and a [bool] parameter then throws mid-run,
# which loses every check after it, including the ones that would have failed.
# Anything truthy that came back counts as a pass; an empty result does not.
function Check([string]$name, $Ok, [string]$detail) {
    $ok = if ($Ok -is [bool]) { $Ok } else { @($Ok | Where-Object { $_ }).Count -gt 0 }
    $checks[$name] = $ok
    Say ("CHECK {0,-28} {1}  {2}" -f $name, $(if ($ok) { "pass" } else { "FAIL" }), $detail)
}
function Flush { $lines | Set-Content $outPath -Encoding UTF8 }
trap { Say "EXCEPTION: $_"; Say $_.ScriptStackTrace; Flush; exit 1 }

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class Win {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public const uint WM_CLOSE = 0x0010;

  // Every visible top-level window, with the class and owning pid, so a
  // "nothing appeared" check can say what did appear when it fails.
  public static List<string> VisibleWindows() {
    var found = new List<string>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      uint pid; GetWindowThreadProcessId(h, out pid);
      var cls = new StringBuilder(256); GetClassName(h, cls, cls.Capacity);
      var txt = new StringBuilder(256); GetWindowText(h, txt, txt.Capacity);
      found.Add(pid + "\t" + cls + "\t" + txt);
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@

function Stop-WinRemap {
    Get-Process winremap -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
}

# Launched the way Explorer launches it: no console anywhere up the tree, so
# AttachConsole(ATTACH_PARENT_PROCESS) finds nothing and the dialog path runs
# (ADR 0029). Start-Process would hand it this script's console instead, and
# the message would quietly go to stdout - passing the test for the wrong
# reason.
function Start-Detached([string]$CommandLine) {
    $r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $CommandLine }
    return $r.ProcessId
}

function Get-DialogsOf([int]$ProcId) {
    $out = @()
    foreach ($w in [Win]::VisibleWindows()) {
        $parts = $w -split "`t"
        if ([int]$parts[0] -eq $ProcId -and $parts[1] -eq "#32770") { $out += $w }
    }
    return $out
}

# The dialog is modal and the process waits in MessageBoxW until it is
# dismissed, so read it, then close it and let the process exit.
function Read-AndCloseDialog([int]$ProcId, [int]$TimeoutSec = 20) {
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $desc = [System.Windows.Automation.TreeScope]::Descendants
    $any = [System.Windows.Automation.Condition]::TrueCondition
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $cond = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $ProcId)
        foreach ($e in $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) {
            if ($e.Current.ClassName -ne "#32770") { continue }
            $text = @()
            foreach ($t in $e.FindAll($desc, $any)) { if ($t.Current.Name) { $text += $t.Current.Name } }
            [Win]::PostMessage([IntPtr]$e.Current.NativeWindowHandle, [Win]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
            return ($text -join " | ")
        }
        Start-Sleep -Milliseconds 500
    }
    return ""
}

Say ("powershell " + $PSVersionTable.PSVersion)
Say ("exe: " + $exe + "  " + (Get-Item $exe).Length + " bytes")
Stop-WinRemap

# --- the command line, with output redirected into this script -------------
$version = (& $exe --version 2>&1 | Out-String).Trim()
$versionCode = $LASTEXITCODE
Say ("--version -> exit=$versionCode  '" + $version + "'")
Check "version-prints" ($versionCode -eq 0 -and $version -match "(?i)winremap\s+\d+\.\d+\.\d+") `
    "a redirected --version reaches the caller (v0.2 A-5)"

$help = (& $exe --help 2>&1 | Out-String)
$helpCode = $LASTEXITCODE
Say ("--help -> exit=$helpCode  " + $help.Length + " chars")
Check "help-prints" ($helpCode -eq 0 -and $help -match "--config" -and $help -match "--lang") `
    "a redirected --help reaches the caller (v0.2 A-4)"

# Redirected to a file rather than to this script: what A-6 is really about is
# that a launcher-supplied stdout handle wins over both the console and the
# dialog. -RedirectStandardOutput hands the child exactly that handle, and
# unlike `cmd /c "... > file"` it does not depend on how PowerShell rewrites
# quotes on the way to cmd.
$helpFile = "C:\Test\help.txt"
Remove-Item $helpFile -Force -ErrorAction SilentlyContinue
Start-Process $exe -ArgumentList '--help' -NoNewWindow -Wait -RedirectStandardOutput $helpFile
[string]$fileText = if (Test-Path $helpFile) { (Get-Content $helpFile) -join "`n" } else { "" }
Say ("help.txt: " + $fileText.Length + " chars")
Check "help-redirects-to-file" ($fileText -match "--config") `
    "--help redirected to a file lands in the file, with no dialog (v0.2 A-6)"

# --- a config that is not there --------------------------------------------
$missing = (& $exe --config C:\Test\no-such-config.toml 2>&1 | Out-String)
$missingCode = $LASTEXITCODE
Say ("missing config -> exit=$missingCode")
Say ("  " + ($missing -replace "\r?\n", " / ").Trim())
Check "missing-config-errors" ($missingCode -ne 0 -and $missing -match "no-such-config\.toml") `
    "a config path that does not exist fails, naming the path (v0.1 smoke)"

# --- launched with no terminal: the message must become a dialog -----------
$brokenPid = Start-Detached "`"$exe`" --config C:\Test\broken.toml --lang en"
Say ("broken config, detached: pid=" + $brokenPid)
$brokenText = Read-AndCloseDialog $brokenPid
Say ("  dialog: " + $brokenText)
Check "broken-config-dialog" ($brokenText -match "broken\.toml" -and $brokenText -match "line \d+") `
    "a silent launch shows the error in a dialog, with the line number (v0.2 A-8)"

$badArgPid = Start-Detached "`"$exe`" --nosuch --lang en"
Say ("unknown argument, detached: pid=" + $badArgPid)
$badArgText = Read-AndCloseDialog $badArgPid
Say ("  dialog: " + $badArgText)
Check "unknown-argument-dialog" ($badArgText -match "nosuch") `
    "an unknown argument shows a dialog naming it (v0.2 A-9)"

# --- a normal silent launch shows nothing at all ---------------------------
Stop-WinRemap
$before = [Win]::VisibleWindows().Count
$quietPid = Start-Detached "`"$exe`" --config C:\Test\minimal.toml --lang en"
Start-Sleep -Seconds 6
$proc = Get-Process -Id $quietPid -ErrorAction SilentlyContinue
$mine = @()
foreach ($w in [Win]::VisibleWindows()) {
    $parts = $w -split "`t"
    if ($proc -and [int]$parts[0] -eq $quietPid) { $mine += $w }
}
Say ("silent launch: pid=$quietPid resident=" + [bool]$proc + " visible windows of it=" + $mine.Count)
foreach ($w in $mine) { Say ("  " + $w) }
Check "stays-resident" ([bool]$proc) "minimal.toml starts and stays resident (v0.1 smoke)"
Check "silent-launch-no-window" ($mine.Count -eq 0) `
    "no console and no window from a double-click style launch (v0.2 A-1, B0-11)"

Stop-WinRemap
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
Say ""
Say ("RESULT: {0} of {1} checks passed" -f ($checks.Count - $failed.Count), $checks.Count)
if ($failed.Count) { Say ("FAILED: " + ($failed -join ', ')) }
Flush
exit $(if ($failed.Count) { 1 } else { 0 })
