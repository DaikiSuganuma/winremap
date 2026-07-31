<#
.SYNOPSIS
    Runs WinRemap's UI test checks on the VMware guest (host-side entry).

.DESCRIPTION
    Standard flow, once per check (docs/05_ui-test-automation.md):
      1. revert the guest to the golden snapshot, boot it and wait until it can
         take an interactive program, so no resident WinRemap or global hook is
         carried over from the previous check
      2. build the binary the check asks for and copy it, with the fixtures and
         the guest-side scripts, to C:\Test on the guest
      3. run the check's script in the guest's session 1 through vmrun
         -interactive (UI automation needs an interactive desktop)
      4. copy its result file back and read the verdict off its own tally

    Every check drives the app through the Windows App Development CLI
    (`winapp`) or a plain UI Automation client, and decides for itself. There
    is no LLM in the loop: the agent-driven scenarios this suite started with
    were retired in v0.7 once all five had been ported (ADR 0064), because the
    ported checks decide the same things about six times faster and, unlike the
    agent, decide them the same way twice.

    Checks marked NeedsInject are built with the default-off `test-inject`
    feature (ADR 0053); the others run the same shape of binary that ships.
    Launches pass --lang en so the assertions can name UI strings regardless of
    the guest's locale.

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

    -DumpUia asserts nothing: it opens the settings and log windows and prints
    their UI Automation trees, which is where the names the checks look for are
    read from. Re-run it whenever the GUI changes.

.EXAMPLE
    .\run-vm-ui-test.ps1
    .\run-vm-ui-test.ps1 -Check 05-remap-notepad -NoRevert
    .\run-vm-ui-test.ps1 -Check 01-settings-window,04-log-window
    .\run-vm-ui-test.ps1 -DumpUia
    .\run-vm-ui-test.ps1 -VmConfig test-vm.spare.json
#>

param(
    # One of the names in $checks below, a comma-separated subset, or "all".
    # -Scenario still works: that is what this parameter was called while the
    # suite was agent-driven, and it is in the older acceptance checklists.
    [Alias("Scenario")]
    [string]$Check = "all",
    # windows-utility's guest-command entry point, used for the revert. It
    # provides the environment; what runs in it — the binary, the fixtures, the
    # checks — is ours.
    [string]$EntryScript = "D:\Projects\GitLab\windows-utility\test-vm\scripts\run-in-vm-vmware.ps1",
    [string]$Snapshot = "ready",
    # Reuses the running guest as-is. For iterating on a check only: results
    # are not reproducible once a check has left state behind.
    [switch]$NoRevert,
    [switch]$SkipBuild,
    # Asserts nothing. Opens the settings and log windows and prints what they
    # expose to UI Automation, so the names the checks look for can be read off
    # a machine instead of guessed. Run this after changing the GUI.
    [switch]$DumpUia,
    # File name under this repository's .secrets (or a full path) naming the VM
    # to run against. The default is this suite's own guest, winremap-test.
    [string]$VmConfig = "test-vm.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
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
# check launched into that gap fails looking like the app.
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
    #
    # Neither is the ordinary target\release, on purpose. A developer running
    # WinRemap from their own build holds that file open, and Windows will not
    # let cargo replace a running executable - the build then fails with
    # "access denied" and the suite looks broken while nothing is (2026-07-29).
    $targetDir = if ($TestInject) { "target\ui-test-inject" } else { "target\ui-release" }
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

# Everything the guest needs, flattened into C:\Test. The guest side is listed
# by directory rather than by name on purpose: the hand-kept list this replaced
# went stale every time a check was added, and a check whose script was never
# copied fails as "the result file is missing" — an ERROR that says nothing
# about what is wrong.
#
# Flat on the guest matters: the check scripts dot-source their helpers as
# "$PSScriptRoot\ui-helpers.ps1", which only resolves if everything landed in
# one directory.
function Copy-Payload {
    param([string]$Exe)
    Invoke-Vmrun createDirectoryInGuest $script:vm.VmxPath $guestDir | Out-Null
    $payload = @($Exe)
    # minimal.toml is what the display checks read back; personal-ja.toml is
    # the one with more than one keymap, macros and a [macro] section, which
    # 00-regression needs (v0.2 B1-3 named it suganuma.toml, since renamed).
    $payload += (Join-Path $repoRoot "examples\minimal.toml")
    $payload += (Join-Path $repoRoot "examples\personal-ja.toml")
    $payload += @(Get-ChildItem (Join-Path $PSScriptRoot "fixtures\*.toml") | ForEach-Object { $_.FullName })
    $payload += @(Get-ChildItem (Join-Path $PSScriptRoot "guest\*.ps1") | ForEach-Object { $_.FullName })
    foreach ($local in $payload) {
        $guest = Join-Path $guestDir (Split-Path $local -Leaf)
        Invoke-Vmrun copyFileFromHostToGuest $script:vm.VmxPath $local $guest | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "copying $local to the guest failed" }
    }
    Write-Host "  payload copied to $guestDir ($($payload.Count) files)" -ForegroundColor Gray
}

# Evidence for a check that failed or hung: the guest is reverted straight
# afterwards, so anything not copied out now is gone.
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
# guest\promote-tray-icon.ps1 for why the checks cannot open that flyout.
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

# The suite. Every entry is a guest-side script that drives the app and decides
# for itself; the name is the script's own stem, so a check and its file are
# never two things to keep in step.
#
# Each one writes CHECK/RESULT/FAILED lines to a file in the guest, because
# vmrun does not carry guest stdout back: a script that dies silently would
# otherwise look exactly like one that never ran.
$checks = [ordered]@{
    # Runs before the rest: if the windows expose nothing, every check after it
    # fails for the same reason and the summary says so nine times.
    #
    # It is also the suite's control implementation — the only one that reads
    # the app through a plain UI Automation client rather than through winapp.
    # That is what tells "winapp is wrong" apart from "the app is wrong", and
    # both Phase 2 and Phase 4 of the migration produced exactly that kind of
    # false alarm (ADR 0064). Keep it even though 01 and 02 overlap it.
    "00-uia-actuation" = @{ Script = "dump-uia.ps1"; Result = "uia-dump.txt"; NeedsTray = $true }
    # No tray, no windows: the command line and what a silent launch does.
    "00-cli-smoke"     = @{ Script = "cli-smoke.ps1"; Result = "cli-smoke.txt"; NeedsTray = $false }
    # The v0.1〜v0.3 regression items that are machine-checkable
    # (docs/v0.5/03_acceptance-checklist.md §5).
    "00-regression"    = @{ Script = "regression-checks.ps1"; Result = "regression-checks.txt"; NeedsTray = $true }
    # The log window's two views (ADR 0057). Needs the test-inject build: the
    # keys are sent with keybd_event, and a shipped build passes injections
    # through, so there would be no decision to log. The screenshots are
    # evidence for the half no assertion covers — whether it reads well.
    "00-log-view"      = @{ Script = "log-view.ps1"; Result = "log-view.txt"; NeedsTray = $true; NeedsInject = $true
        Files = @("log-view-simple.png", "log-view-detailed.png")
    }
    # 01〜05 were the agent-driven scenarios until v0.7 (ADR 0064). Same names,
    # same subjects, no LLM.
    "01-settings-window" = @{ Script = "01-settings-window.ps1"; Result = "01-settings-window.txt"; NeedsTray = $true }
    "02-config-display"  = @{ Script = "02-config-display.ps1"; Result = "02-config-display.txt"; NeedsTray = $true }
    "03-tray-actions"    = @{ Script = "03-tray-actions.ps1"; Result = "03-tray-actions.txt"; NeedsTray = $true }
    "04-log-window"      = @{ Script = "04-log-window.ps1"; Result = "04-log-window.txt"; NeedsTray = $true }
    # The other check that needs the test-inject build: it presses a real chord,
    # and every way of pressing one from a script is an injection, which a
    # shipped build passes through untouched (ADR 0053). No tray — it never
    # opens WinRemap's own windows.
    "05-remap-notepad"   = @{ Script = "05-remap-notepad.ps1"; Result = "05-remap-notepad.txt"; NeedsTray = $false; NeedsInject = $true }
    # v0.7 (plan section 3.5): settles the v0.5 carry-over — whether the app
    # switched to reaches the foreground line while the log window is open, and
    # whether a rule scoped to that app is still chosen. The second half is why
    # this one needs the test-inject build: it presses a chord.
    "06-foreground-line" = @{ Script = "06-foreground-line.ps1"; Result = "06-foreground-line.txt"; NeedsTray = $true; NeedsInject = $true
        # The pixels behind the verdict: what the window was showing while the
        # tree said nothing had arrived.
        Files = @("foreground-line-switched.png", "foreground-line-back.png")
    }
}

function Invoke-GuestCheck {
    param([string]$ScriptName, [string]$ResultName, [bool]$Verbose, [string[]]$Files = @())
    $ps = "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
    Invoke-Vmrun runProgramInGuest $script:vm.VmxPath -interactive $ps `
        "-NoProfile" "-ExecutionPolicy" "Bypass" "-File" "$guestDir\$ScriptName" | Out-Null
    # Screenshots and the like: evidence the verdict does not carry. Copied
    # before the result file is read, so they survive a failing check.
    foreach ($name in $Files) {
        $local = Join-Path $env:TEMP "winremap-$name"
        Remove-Item $local -Force -ErrorAction SilentlyContinue
        Invoke-Vmrun copyFileFromGuestToHost $script:vm.VmxPath "$guestDir\$name" $local | Out-Null
        if (Test-Path $local) { Write-Host "  saved: $local" -ForegroundColor Gray }
    }
    $dump = Join-Path $env:TEMP "winremap-$ResultName"
    Remove-Item $dump -Force -ErrorAction SilentlyContinue
    Invoke-Vmrun copyFileFromGuestToHost $script:vm.VmxPath "$guestDir\$ResultName" $dump | Out-Null
    if (-not (Test-Path $dump)) { return "ERROR" }
    $lines = Get-Content $dump -Encoding UTF8
    if ($Verbose) { $lines | Write-Host }
    else { $lines | Where-Object { $_ -match '^(CHECK|RESULT|FAILED|EXCEPTION)' } | Write-Host }
    Write-Host "  saved: $dump" -ForegroundColor Gray
    # The dump is the record; its own tally is the verdict.
    if ($lines | Where-Object { $_ -match '^FAILED:' }) { return "FAIL" }
    if ($lines | Where-Object { $_ -match '^RESULT: \d+ of \d+ checks passed' }) { return "PASS" }
    return "ERROR"
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
    $verdict = Invoke-GuestCheck -ScriptName "dump-uia.ps1" -ResultName "uia-dump.txt" -Verbose $true
    Write-Host "`n  $verdict" -ForegroundColor $(if ($verdict -eq "PASS") { "Green" } else { "Red" })
    exit $(if ($verdict -eq "PASS") { 0 } else { 1 })
}

# One check, from a clean guest: build what it asks for, revert, copy, promote
# the tray icon if it needs one, run it, read its verdict.
function Invoke-Check {
    param([string]$Name)
    $check = $checks[$Name]
    Write-Host "`n=== $Name ===" -ForegroundColor Cyan
    # .Contains, not .Key: StrictMode makes reading an absent key an error, so
    # every optional field has to be asked for before it is read.
    $inject = $check.Contains("NeedsInject") -and $check.NeedsInject
    $exe = Build-Binary -TestInject $inject
    if (-not $NoRevert) { Reset-Guest }
    Copy-Payload -Exe $exe
    if ($check.NeedsTray -and -not (Set-TrayIconPromoted)) { return "SETUP FAILED" }
    $files = if ($check.Contains("Files")) { $check.Files } else { @() }
    return Invoke-GuestCheck -ScriptName $check.Script -ResultName $check.Result -Verbose $false -Files $files
}

# Split on whitespace as well as commas: PowerShell binds a comma-separated
# argument as an array and joins it with spaces on the way into [string], so
# splitting on commas alone looked for one check with a very long name.
$requested = @(
    if ($Check -eq "all") { $checks.Keys }
    else { $Check -split '[,\s]+' | Where-Object { $_.Trim() } | ForEach-Object { $_.Trim() } }
)
$unknown = @($requested | Where-Object { -not $checks.Contains($_) })
if ($unknown.Count) {
    throw "no such check: $($unknown -join ', ')  (known: $($checks.Keys -join ', '))"
}

$results = [ordered]@{}
foreach ($name in $requested) {
    $verdict = Invoke-Check -Name $name
    # A check that fails or hangs leaves nothing behind once the guest is
    # reverted. Grab the screen and the guest-side log while they still exist.
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
