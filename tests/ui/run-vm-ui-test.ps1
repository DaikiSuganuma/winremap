<#
.SYNOPSIS
    Runs WinRemap's UI test scenarios on the VMware guest (host-side entry).

.DESCRIPTION
    Standard flow, once per scenario (docs/05_ui-test-automation.md):
      1. revert the guest to the golden snapshot and boot it, so no resident
         WinRemap or global hook is carried over from the previous scenario
      2. build the binary the scenario asks for and copy it, with
         examples/minimal.toml, to C:\Test on the guest
      3. run `claude -p <scenario prompt>` in the guest's session 1 through
         windows-utility's run-in-vm-vmware.ps1 (UI automation needs an
         interactive desktop)
      4. read the verdict from the agent's final line

    Scenarios whose prompt mentions --accept-injected are built with the
    default-off `test-inject` feature (ADR 0053); the others run the same
    shape of binary that ships. Launches pass --lang en so the prompts can
    name UI strings regardless of the guest's locale.

    VM connection details are read from windows-utility's .secrets\test-vm.json
    and are never echoed.

.EXAMPLE
    .\run-vm-ui-test.ps1
    .\run-vm-ui-test.ps1 -Scenario 05-remap-notepad -NoRevert
#>

param(
    # Scenario file stem under .\scenarios, or "all".
    [string]$Scenario = "all",
    # windows-utility's guest-command entry point; its repo root holds .secrets.
    [string]$EntryScript = "D:\Projects\GitLab\windows-utility\test-vm\scripts\run-in-vm-vmware.ps1",
    [string]$Snapshot = "ready",
    [int]$TimeoutMin = 15,
    # Reuses the running guest as-is. For iterating on a prompt only: results
    # are not reproducible once a scenario has left state behind.
    [switch]$NoRevert,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$scenarioDir = Join-Path $PSScriptRoot "scenarios"
$guestDir = "C:\Test"

function Resolve-Vmrun {
    $bases = @(
        "$env:ProgramFiles\VMware\VMware Workstation",
        "${env:ProgramFiles(x86)}\VMware\VMware Workstation"
    )
    foreach ($base in $bases) {
        $exe = Join-Path $base "vmrun.exe"
        if (Test-Path $exe) { return $exe }
    }
    throw "vmrun.exe not found. Install VMware Workstation (Hyper-V cannot run the OpenGL GUI)."
}

function Get-VmConfig {
    if (-not (Test-Path $EntryScript)) {
        throw "Entry script not found: $EntryScript (pass -EntryScript <path to run-in-vm-vmware.ps1>)"
    }
    $utilityRoot = Split-Path (Split-Path (Split-Path $EntryScript -Parent) -Parent) -Parent
    $configPath = Join-Path $utilityRoot ".secrets\test-vm.json"
    if (-not (Test-Path $configPath)) {
        throw "VM connection details not found: $configPath (run windows-utility's setup-01-vmware.ps1 first)"
    }
    Get-Content $configPath -Raw -Encoding UTF8 | ConvertFrom-Json
}

# Guest credentials go on the command line, so callers must not echo the args.
function Invoke-Vmrun {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$VmrunArgs)
    & $script:vmrun -T ws -gu $script:vm.UserName -gp $script:vm.Password @VmrunArgs 2>&1
}

# The golden snapshot was taken while an earlier test run was still live, so
# reverting resumes that run's claude and MCP processes. They keep
# C:\Setup\run-output.log open, our own run's output never lands in it, and
# the scenario times out with nothing to show. Clear them before starting.
# Harmless when the snapshot is clean; drop this once it is rebuilt.
function Clear-StaleRun {
    $ps = "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
    # powershell.exe is deliberately absent from the list: this command is one.
    $script = "Get-Process node,CalculatorApp,cmd,winremap,notepad -ErrorAction SilentlyContinue | " +
    "Stop-Process -Force -ErrorAction SilentlyContinue; " +
    "Remove-Item 'C:\Setup\run-output.log','C:\Setup\run-done.txt' -Force -ErrorAction SilentlyContinue"
    Invoke-Vmrun runProgramInGuest $script:vm.VmxPath $ps "-NoProfile" "-Command" $script | Out-Null
    Write-Host "  cleared any run left over in the snapshot" -ForegroundColor Gray
}

function Reset-Guest {
    Write-Host "  reverting to snapshot '$Snapshot'..." -ForegroundColor Gray
    & $script:vmrun -T ws revertToSnapshot $script:vm.VmxPath $Snapshot 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "revertToSnapshot failed (snapshot '$Snapshot' missing?)" }
    & $script:vmrun -T ws start $script:vm.VmxPath 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "starting the VM failed" }

    # Tools answering with valid credentials is the first moment the guest can
    # take files; auto-logon completes around the same time.
    $deadline = (Get-Date).AddMinutes(5)
    while ((Get-Date) -lt $deadline) {
        Invoke-Vmrun listProcessesInGuest $script:vm.VmxPath | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  guest is up" -ForegroundColor Gray
            # The desktop needs a moment more before UI automation can attach.
            Start-Sleep -Seconds 15
            Clear-StaleRun
            return
        }
        Start-Sleep -Seconds 5
    }
    throw "the guest did not become ready within 5 minutes"
}

function Build-Binary {
    param([bool]$TestInject)
    # Separate target dirs: both binaries can be needed in one run, and they
    # would otherwise overwrite each other at target\release\winremap.exe.
    $targetDir = if ($TestInject) { "target\ui-test-inject" } else { "target" }
    $exe = Join-Path $repoRoot "$targetDir\release\winremap.exe"
    if ($SkipBuild) {
        if (-not (Test-Path $exe)) { throw "-SkipBuild was given but $exe does not exist" }
        return $exe
    }
    $label = if ($TestInject) { "release + test-inject" } else { "release" }
    Write-Host "  building ($label)..." -ForegroundColor Gray
    Push-Location $repoRoot
    try {
        # Out-Host, not the pipeline: anything cargo writes to stdout would
        # otherwise be returned alongside the path this function yields.
        if ($TestInject) {
            cargo build --release --features test-inject --target-dir $targetDir | Out-Host
        }
        else {
            cargo build --release --target-dir $targetDir | Out-Host
        }
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }
    finally { Pop-Location }
    return $exe
}

function Copy-Payload {
    param([string]$Exe)
    Invoke-Vmrun createDirectoryInGuest $script:vm.VmxPath $guestDir | Out-Null
    foreach ($pair in @(
            @{ Local = $Exe; Guest = "$guestDir\winremap.exe" },
            # minimal.toml is what the display scenarios read back; uitest.toml
            # is the fixture the remap scenario needs (see its header).
            @{ Local = (Join-Path $repoRoot "examples\minimal.toml"); Guest = "$guestDir\minimal.toml" },
            @{ Local = (Join-Path $PSScriptRoot "fixtures\uitest.toml"); Guest = "$guestDir\uitest.toml" },
            @{ Local = (Join-Path $PSScriptRoot "guest\promote-tray-icon.ps1"); Guest = "$guestDir\promote-tray-icon.ps1" }
        )) {
        Invoke-Vmrun copyFileFromHostToGuest $script:vm.VmxPath $pair.Local $pair.Guest | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "copying $($pair.Local) to the guest failed" }
    }
    Write-Host "  payload copied to $guestDir" -ForegroundColor Gray
}

# A scenario that fails or hangs leaves nothing behind once the guest is
# reverted, and the agent's own output is the first thing missing when it
# hangs. Grab the screen and the guest-side log while they still exist.
function Save-Diagnostics {
    param([string]$Name)
    $dir = Join-Path $env:TEMP "winremap-ui-test"
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    $shot = Join-Path $dir "$Name.png"
    $log = Join-Path $dir "$Name.log"
    Invoke-Vmrun captureScreen $script:vm.VmxPath $shot | Out-Null
    Invoke-Vmrun copyFileFromGuestToHost $script:vm.VmxPath "C:\Setup\run-output.log" $log | Out-Null
    Write-Host "  diagnostics: $dir" -ForegroundColor Yellow
}

# Puts WinRemap's icon on the taskbar instead of in the overflow flyout; see
# guest\promote-tray-icon.ps1 for why the scenarios cannot open that flyout.
function Set-TrayIconPromoted {
    $ps = "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
    Invoke-Vmrun runProgramInGuest $script:vm.VmxPath -interactive $ps `
        "-NoProfile" "-ExecutionPolicy" "Bypass" "-File" "$guestDir\promote-tray-icon.ps1" | Out-Null
    Write-Host "  tray icon promoted out of the overflow" -ForegroundColor Gray
}

function Invoke-Scenario {
    param([System.IO.FileInfo]$File)

    $prompt = (Get-Content $File.FullName -Raw).TrimEnd()
    # The guest's PATH does not include npm's global bin in a non-login shell,
    # and the token lives in the User environment (never in this repo).
    # Set-Location matters: vmrun starts the command in C:\Windows\System32,
    # and the terminator MCP server never becomes healthy when spawned with
    # that as its working directory — claude then hangs before its first tool
    # call, with no output at all.
    $command = @"
`$env:CLAUDE_CODE_OAUTH_TOKEN = [Environment]::GetEnvironmentVariable('CLAUDE_CODE_OAUTH_TOKEN','User')
`$env:Path = `$env:Path + ';' + `$env:APPDATA + '\npm'
Set-Location `$env:USERPROFILE
`$prompt = @'
$prompt
'@
claude -p `$prompt --dangerously-skip-permissions
"@

    # The entry script reports through Write-Host, so the guest's output only
    # reaches the pipeline with the information stream redirected (6>&1).
    $output = & $EntryScript -Command $command -TimeoutMin $TimeoutMin 6>&1 2>&1 | ForEach-Object {
        Write-Host $_
        $_
    }
    # A timeout or a failed launch is a failed scenario whatever the text
    # says; the entry script reports that through its exit code.
    if ($LASTEXITCODE -ne 0) { return "ERROR" }

    # Only a line that is nothing but the verdict counts. The entry script
    # echoes the command it runs — prompt included — so a substring match
    # would find the "print exactly PASS" of the instructions themselves.
    # -cmatch keeps prose like "passed" out of it.
    $verdict = "NO VERDICT"
    foreach ($line in $output) {
        if ("$line" -cmatch '^\s*(?:\|\s*)?(PASS|FAIL)[.\s]*$') { $verdict = $Matches[1] }
    }
    return $verdict
}

# ---------------------------------------------------------------------------

$script:vmrun = Resolve-Vmrun
$script:vm = Get-VmConfig

# The @() must wrap the whole statement: assigning from an if unwraps a
# single match back to a scalar, and .Count then fails under StrictMode.
$files = @(
    if ($Scenario -eq "all") {
        # _-prefixed files are diagnostics of the harness itself, run by name.
        Get-ChildItem (Join-Path $scenarioDir "*.txt") |
            Where-Object { $_.Name -notlike "_*" } | Sort-Object Name
    }
    else {
        Get-ChildItem (Join-Path $scenarioDir "$Scenario.txt")
    }
)
if ($files.Count -eq 0) { throw "no scenario matched '$Scenario' in $scenarioDir" }

$results = [ordered]@{}
foreach ($file in $files) {
    $name = $file.BaseName
    Write-Host "`n=== $name ===" -ForegroundColor Cyan
    # The flag only exists in a test-inject build, so the prompt naming it
    # decides which binary this scenario needs (ADR 0053).
    $needsInject = (Get-Content $file.FullName -Raw) -match '--accept-injected'
    $exe = Build-Binary -TestInject $needsInject
    if (-not $NoRevert) { Reset-Guest }
    Copy-Payload -Exe $exe
    # Only the scenarios that use the tray need it, and it costs ~20 seconds.
    if ((Get-Content $file.FullName -Raw) -match 'notification area|tray') {
        if ((Get-Content $file.FullName -Raw) -notmatch 'Do not touch the notification area') {
            Set-TrayIconPromoted
        }
    }
    $verdict = Invoke-Scenario -File $file
    if ($verdict -ne "PASS") { Save-Diagnostics -Name $name }
    $results[$name] = $verdict
}

if (-not $NoRevert) {
    # Leaves no resident WinRemap or global hook behind in the guest.
    Write-Host "`ncleaning up..." -ForegroundColor Gray
    & $script:vmrun -T ws revertToSnapshot $script:vm.VmxPath $Snapshot 2>&1 | Out-Null
}

Write-Host "`n=== summary ===" -ForegroundColor Cyan
foreach ($name in $results.Keys) {
    $verdict = $results[$name]
    $color = if ($verdict -eq "PASS") { "Green" } else { "Red" }
    Write-Host ("  {0,-24} {1}" -f $name, $verdict) -ForegroundColor $color
}
if ($results.Values -contains "PASS" -and -not ($results.Values | Where-Object { $_ -ne "PASS" })) {
    exit 0
}
exit 1
