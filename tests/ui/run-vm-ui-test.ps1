<#
.SYNOPSIS
    Runs WinRemap's UI test scenarios on the VMware guest (host-side entry).

.DESCRIPTION
    Standard flow, once per scenario (docs/05_ui-test-automation.md):
      1. revert the guest to the golden snapshot, boot it and wait until it can
         take an interactive program, so no resident WinRemap or global hook is
         carried over from the previous scenario
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

    One project, one VM: this suite owns winremap-test, and its connection
    details live in this repository's own .secrets\test-vm.json (git-ignored,
    never echoed). Every call into windows-utility names that file with
    -ConfigPath. Nothing here reads or writes windows-utility's .secrets —
    that repository's test-vm.json is a shared default slot, and a run that
    repoints it sends another project's commands to this guest.

    Create the VM once with (from windows-utility\test-vm\scripts):
      $cfg = "D:\Projects\GitHub\winremap\.secrets\test-vm.json"
      .\clone-vm-vmware.ps1 -NewVMName winremap-test -Snapshot ready `
          -SourceConfigPath ..\..\.secrets\test-vm.template-win11.json `
          -NewConfigPath $cfg
      .\snapshot-golden-vmware.ps1 -ConfigPath $cfg -AppProcessNames winremap

    -AppProcessNames matters every time the golden snapshot is retaken: a
    WinRemap left resident in it comes back on every revert, keeps its log file
    open, and the next run times out silently with nothing to show.

    -DumpUia runs no scenario at all: it opens the settings and log windows and
    prints their UI Automation trees, which is where the selectors the
    scenarios name are read from. Re-run it whenever the GUI changes.

.EXAMPLE
    .\run-vm-ui-test.ps1
    .\run-vm-ui-test.ps1 -Scenario 05-remap-notepad -NoRevert
    .\run-vm-ui-test.ps1 -DumpUia
    .\run-vm-ui-test.ps1 -VmConfig test-vm.spare.json
#>

param(
    # Scenario file stem under .\scenarios, or "all".
    [string]$Scenario = "all",
    # windows-utility's guest-command entry point. It provides the environment;
    # what runs in it — the binary, the fixtures, the scenarios — is ours.
    [string]$EntryScript = "D:\Projects\GitLab\windows-utility\test-vm\scripts\run-in-vm-vmware.ps1",
    [string]$Snapshot = "ready",
    # Generous on purpose: a green run measured 11:54 / 14:05 / 3:34 / 12:23 /
    # 1:38, so anything near 15 turns a slow scenario into a false ERROR — and
    # a false ERROR costs more than a slow pass, because it sends someone
    # looking for a defect that is not there. A scenario can ask for its own
    # budget with `# timeout: <minutes>`.
    [int]$TimeoutMin = 25,
    # Reuses the running guest as-is. For iterating on a prompt only: results
    # are not reproducible once a scenario has left state behind.
    [switch]$NoRevert,
    [switch]$SkipBuild,
    # Runs no scenario. Opens the settings and log windows and prints what they
    # expose to UI Automation, so the selectors in the scenarios can be read
    # off a machine instead of guessed. Run this after changing the GUI.
    [switch]$DumpUia,
    # File name under this repository's .secrets (or a full path) naming the VM
    # to run against. The default is this suite's own guest, winremap-test.
    [string]$VmConfig = "test-vm.json"
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
    $configPath = if ([System.IO.Path]::IsPathRooted($VmConfig)) { $VmConfig }
    else { Join-Path $repoRoot ".secrets\$VmConfig" }
    if (-not (Test-Path $configPath)) {
        throw "VM connection details not found: $configPath (create the guest with " +
        "windows-utility's clone-vm-vmware.ps1 -NewConfigPath <this path>; see the header)"
    }
    $script:vmConfigPath = $configPath
    Get-Content $configPath -Raw -Encoding UTF8 | ConvertFrom-Json
}

# `vmrun start` launches the VMware Workstation GUI when it is not already
# running, and that GUI outlives vmrun by hours. Anything that waits on more
# than vmrun's own exit therefore waits for the user to quit VMware, and the
# run sits at "reverting to snapshot" with the guest up and healthy and nothing
# to show for it. Two ways in, both taken here:
#
#   - `& vmrun ... | Out-Null` waits for the stdout pipe to reach EOF, and the
#     GUI inherits the write end
#   - `Start-Process -Wait` waits for the process *and its descendants*
#
# So: start it ourselves, hand the child fresh pipes instead of this shell's
# handles, and wait on that one process handle. The streams are deliberately
# never read — the GUI can hold their write ends open forever. vmrun's own
# output is a line or two, far short of the pipe buffer it would block on.
# Returns the exit code; these calls need no credentials.
function Invoke-VmrunHost {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$VmrunArgs)
    $psi = [System.Diagnostics.ProcessStartInfo]::new($script:vmrun)
    foreach ($arg in @("-T", "ws") + $VmrunArgs) { $psi.ArgumentList.Add($arg) }
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $proc = [System.Diagnostics.Process]::Start($psi)
    $proc.WaitForExit()
    $code = $proc.ExitCode
    $proc.Dispose()
    return $code
}

# Guest credentials go on the command line, so callers must not echo the args.
function Invoke-Vmrun {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$VmrunArgs)
    & $script:vmrun -T ws -gu $script:vm.UserName -gp $script:vm.Password @VmrunArgs 2>&1
}

# Reverting, booting and waiting are all -Restore in the entry script, so ask
# it rather than re-implementing them. What we used to do — wait for Tools to
# answer, then sleep a flat 15 seconds — is not the same condition: Tools
# answers minutes before the desktop will accept an interactive program, and a
# scenario launched into that gap fails looking like the app.
#
# The entry script needs a command, and there is nothing useful to run yet:
# the payload is copied after the revert, because the revert would wipe it.
function Reset-Guest {
    Write-Host "  reverting to '$Snapshot' and waiting for the guest..." -ForegroundColor Gray
    & $EntryScript -ConfigPath $script:vmConfigPath -Restore -Snapshot $Snapshot `
        -Command "Write-Output 'guest is ready'" -TimeoutMin 5 6>&1 2>&1 |
        ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }
    if ($LASTEXITCODE -ne 0) { throw "reverting to '$Snapshot' failed" }
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
            @{ Local = (Join-Path $PSScriptRoot "guest\promote-tray-icon.ps1"); Guest = "$guestDir\promote-tray-icon.ps1" },
            @{ Local = (Join-Path $PSScriptRoot "guest\dump-uia.ps1"); Guest = "$guestDir\dump-uia.ps1" }
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
    $result = Join-Path $env:TEMP "winremap-promote-result.txt"

    # Shortly after a revert the interactive session is not always ready to
    # take a program, and vmrun says so only through its exit code — the run
    # then loses its whole timeout to a tray icon nothing ever promoted. Check
    # the code, and give the guest one more chance before giving up.
    foreach ($attempt in 1..2) {
        Invoke-Vmrun runProgramInGuest $script:vm.VmxPath -interactive $ps `
            "-NoProfile" "-ExecutionPolicy" "Bypass" "-File" "$guestDir\promote-tray-icon.ps1" | Out-Null
        $launched = ($LASTEXITCODE -eq 0)

        # vmrun does not bring guest stdout back, so the script leaves its
        # result in a file. Reading it turns a silent no-op into a failure we
        # can see.
        Remove-Item $result -Force -ErrorAction SilentlyContinue
        Invoke-Vmrun copyFileFromGuestToHost $script:vm.VmxPath "$guestDir\promote-result.txt" $result | Out-Null
        $promoted = if (Test-Path $result) { (Get-Content $result -Raw).Trim() } else { "promoted=?" }
        if ($promoted -eq "promoted=1") {
            Write-Host "  tray icon: $promoted" -ForegroundColor Gray
            return $true
        }
        Write-Host "  tray icon: $promoted (attempt $attempt, guest launch ok: $launched)" -ForegroundColor Yellow
        Start-Sleep -Seconds 15
    }
    return $false
}

# The deterministic half of the suite. Everything an agent has to interpret
# lives in the scenarios; this presses buttons through a plain UI Automation
# client and checks the windows actually changed. Through terminator the same
# two buttons landed about half the time, and a check that flaky reports bad
# days as regressions — this one has yet to miss.
function Invoke-UiaChecks {
    param([bool]$Verbose)
    $ps = "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
    Invoke-Vmrun runProgramInGuest $script:vm.VmxPath -interactive $ps `
        "-NoProfile" "-ExecutionPolicy" "Bypass" "-File" "$guestDir\dump-uia.ps1" | Out-Null
    $dump = Join-Path $env:TEMP "winremap-uia-dump.txt"
    Remove-Item $dump -Force -ErrorAction SilentlyContinue
    Invoke-Vmrun copyFileFromGuestToHost $script:vm.VmxPath "$guestDir\uia-dump.txt" $dump | Out-Null
    if (-not (Test-Path $dump)) { return "ERROR" }
    $lines = Get-Content $dump -Encoding UTF8
    if ($Verbose) { $lines | Write-Host }
    else { $lines | Where-Object { $_ -match '^(CHECK|RESULT|FAILED)' } | Write-Host }
    Write-Host "  saved: $dump" -ForegroundColor Gray
    # The dump is the record; its own tally is the verdict.
    if ($lines | Where-Object { $_ -match '^FAILED:' }) { return "FAIL" }
    if ($lines | Where-Object { $_ -match '^RESULT: \d+ of \d+ checks passed' }) { return "PASS" }
    return "ERROR"
}

function Invoke-Scenario {
    param([System.IO.FileInfo]$File, [int]$Minutes)

    # Directive lines configure the run and are not part of the prompt.
    $prompt = ((Get-Content $File.FullName -Raw) -replace '(?m)^#\s*(needs|timeout):.*\r?\n', '').TrimEnd()
    # The guest's PATH does not include npm's global bin in a non-login shell,
    # and the token lives in the User environment (never in this repo).
    # Set-Location matters: vmrun starts the command in C:\Windows\System32,
    # and the terminator MCP server never becomes healthy when spawned with
    # that as its working directory — claude then hangs before its first tool
    # call, with no output at all.
    # The prompt goes in on stdin, not as an argument. Windows PowerShell 5.1
    # mangles a native-command argument containing double quotes: it strips
    # them, and once they are unbalanced the rest of the argument is lost. A
    # 2557-character prompt arrived as 1687 characters, cut mid-sentence at
    # `("Settings" and "Show` — and the agent, given four of seven steps, did
    # those four and stopped without a verdict. It read as the app failing.
    #
    # $OutputEncoding is what PowerShell encodes a pipe to a native command
    # with; left at the console default it turns every non-ASCII character
    # into "?". Both together make what the agent reads byte-identical to the
    # scenario file.
    $command = @"
`$env:CLAUDE_CODE_OAUTH_TOKEN = [Environment]::GetEnvironmentVariable('CLAUDE_CODE_OAUTH_TOKEN','User')
`$env:Path = `$env:Path + ';' + `$env:APPDATA + '\npm'
Set-Location `$env:USERPROFILE
`$OutputEncoding = New-Object System.Text.UTF8Encoding `$false
`$prompt = @'
$prompt
'@
`$prompt | claude -p --dangerously-skip-permissions
"@

    # The entry script reports through Write-Host, so the guest's output only
    # reaches the pipeline with the information stream redirected (6>&1).
    $output = & $EntryScript -ConfigPath $script:vmConfigPath -Command $command `
        -TimeoutMin $Minutes 6>&1 2>&1 | ForEach-Object {
        Write-Host $_
        $_
    }
    # A timeout or a failed launch is a failed scenario whatever the text
    # says; the entry script reports that through its exit code.
    if ($LASTEXITCODE -ne 0) { return "ERROR" }

    # Read the verdict from the guest's own log rather than from the streamed
    # copy above: the entry script stops streaming the moment it sees the
    # completion marker, which can cut off the very last lines — the verdict
    # among them. The file on the guest is always complete by then.
    $log = Join-Path $env:TEMP "winremap-ui-verdict.log"
    Remove-Item $log -Force -ErrorAction SilentlyContinue
    Invoke-Vmrun copyFileFromGuestToHost $script:vm.VmxPath "C:\Setup\run-output.log" $log | Out-Null
    $lines = if (Test-Path $log) { Get-Content $log -Encoding UTF8 } else { $output }

    # Only a line that is nothing but the verdict counts: the agent is asked to
    # end with one, and its own report quotes the instruction ("print exactly
    # PASS"). -cmatch keeps prose like "passed" out of it, and ** ** allows the
    # markdown emphasis the agent sometimes adds.
    $verdict = "NO VERDICT"
    foreach ($line in $lines) {
        if ("$line" -cmatch '^\s*(?:\|\s*)?\**(PASS|FAIL)\**[.\s]*$') { $verdict = $Matches[1] }
    }
    return $verdict
}

# ---------------------------------------------------------------------------

$script:vmrun = Resolve-Vmrun
$script:vm = Get-VmConfig
Write-Host "  VM: $($script:vm.VMName)  ($script:vmConfigPath)" -ForegroundColor Gray

if ($DumpUia) {
    Write-Host "`n=== UIA dump ===" -ForegroundColor Cyan
    $exe = Build-Binary -TestInject $false
    if (-not $NoRevert) { Reset-Guest }
    Copy-Payload -Exe $exe
    if (-not (Set-TrayIconPromoted)) { throw "the tray icon was not promoted" }
    $verdict = Invoke-UiaChecks -Verbose $true
    Write-Host "`n  $verdict" -ForegroundColor $(if ($verdict -eq "PASS") { "Green" } else { "Red" })
    exit $(if ($verdict -eq "PASS") { 0 } else { 1 })
}

# The @() must wrap the whole statement: assigning from an if unwraps a
# single match back to a scalar, and .Count then fails under StrictMode.
$files = @(
    if ($Scenario -eq "all") {
        # _-prefixed files are diagnostics of the harness itself, run by name.
        Get-ChildItem (Join-Path $scenarioDir "*.txt") |
            Where-Object { $_.Name -notlike "_*" } | Sort-Object Name
    }
    else {
        # Comma-separated names run a subset, e.g. -Scenario 02-...,03-...
        # Split on whitespace too: PowerShell binds a comma-separated argument
        # as an array and joins it with spaces on the way into [string], so
        # splitting on commas alone looked for one file with a very long name.
        foreach ($name in $Scenario -split '[,\s]+') {
            Get-ChildItem (Join-Path $scenarioDir "$($name.Trim()).txt")
        }
    }
)
if ($files.Count -eq 0) { throw "no scenario matched '$Scenario' in $scenarioDir" }

$needsTray = @{}
foreach ($file in $files) {
    $needsTray[$file.BaseName] = (Get-Content $file.FullName -Raw) -match '(?m)^#\s*needs:\s*tray\s*$'
}

$results = [ordered]@{}

# Part of a full run, not a separate command to remember: the scenarios check
# what the windows expose, this checks that pressing what they expose works.
if ($Scenario -eq "all") {
    Write-Host "`n=== 00-uia-actuation ===" -ForegroundColor Cyan
    $exe = Build-Binary -TestInject $false
    if (-not $NoRevert) { Reset-Guest }
    Copy-Payload -Exe $exe
    $results["00-uia-actuation"] = if (Set-TrayIconPromoted) { Invoke-UiaChecks -Verbose $false }
    else { "SETUP FAILED" }
}

foreach ($file in $files) {
    $name = $file.BaseName
    Write-Host "`n=== $name ===" -ForegroundColor Cyan
    # The flag only exists in a test-inject build, so the prompt naming it
    # decides which binary this scenario needs (ADR 0053).
    $needsInject = (Get-Content $file.FullName -Raw) -match '--accept-injected'
    $exe = Build-Binary -TestInject $needsInject
    if (-not $NoRevert) { Reset-Guest }
    Copy-Payload -Exe $exe
    # A scenario asks for the tray with a `# needs: tray` directive line.
    # Sniffing the prompt wording for it was silently wrong the moment a
    # scenario was reworded — scenario 02 then looked for an icon nothing had
    # promoted, and blamed the app.
    # A scenario that works the tray menu is far slower than one that does not,
    # because the popup is exposed to UI Automation only sometimes and the
    # agent falls back to raw keystrokes. Rather than give every scenario the
    # slowest one's budget — which would let a genuine hang burn it too — a
    # scenario asks for its own with `# timeout: <minutes>`.
    $minutes = if ((Get-Content $file.FullName -Raw) -match '(?m)^#\s*timeout:\s*(\d+)\s*$') {
        [int]$Matches[1]
    }
    else { $TimeoutMin }
    if ($minutes -ne $TimeoutMin) { Write-Host "  timeout: $minutes min" -ForegroundColor Gray }

    $ready = if ($needsTray[$name]) { Set-TrayIconPromoted } else { $true }
    $verdict = if ($ready) {
        Invoke-Scenario -File $file -Minutes $minutes
    }
    else {
        Write-Host "  skipped: the tray icon was not promoted" -ForegroundColor Red
        "SETUP FAILED"
    }
    if ($verdict -ne "PASS") { Save-Diagnostics -Name $name }
    $results[$name] = $verdict
}

if (-not $NoRevert) {
    # Leaves no resident WinRemap or global hook behind in the guest.
    Write-Host "`ncleaning up..." -ForegroundColor Gray
    Invoke-VmrunHost revertToSnapshot $script:vm.VmxPath $Snapshot | Out-Null
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
