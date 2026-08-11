<#
.SYNOPSIS
    Builds the WinRemap MSIX package layout, and optionally registers, packs
    or signs it (ADR 0060).

.DESCRIPTION
    Three modes, in the order you will want them:

      -Register   Registers the layout folder in place. Requires Developer
                  Mode; needs no certificate and no administrator. This is
                  how you test that a packaged WinRemap actually works.

      -Pack       Produces an unsigned .msix. That is what Partner Center
                  wants — the Store re-signs it, so do not sign it yourself.

      -SelfSign   Packs, then signs with a self-signed certificate so the
                  real install path can be exercised. Importing the
                  certificate into TrustedPeople needs administrator, which
                  is why -Register exists.

.EXAMPLE
    .\build.ps1 -Register
    .\build.ps1 -Pack
#>
[CmdletBinding()]
param(
    [switch]$Register,
    [switch]$Pack,
    [switch]$SelfSign,
    [ValidateSet('release', 'debug')]
    [string]$Configuration = 'release',
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Resolve-Path (Join-Path $here '..\..')
$layout = Join-Path $here 'layout'
if (-not $OutDir) { $OutDir = Join-Path $here 'out' }

if (-not ($Register -or $Pack -or $SelfSign)) {
    throw 'Pick a mode: -Register, -Pack or -SelfSign (see -? for what each does).'
}

# --- Windows SDK -----------------------------------------------------------
# Highest installed version wins; the packaging tools are backward compatible
# and pinning one would break on a machine that has a newer SDK only.
function Get-SdkTool([string]$name) {
    $roots = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
        "$env:ProgramFiles\Windows Kits\10\bin"
    ) | Where-Object { Test-Path $_ }
    $tool = Get-ChildItem -Path $roots -Recurse -Filter $name -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } |
        Sort-Object { [version]($_.FullName -replace '.*\\10\\bin\\([\d.]+)\\.*', '$1') } |
        Select-Object -Last 1
    if (-not $tool) { throw "$name not found. Install the Windows 10/11 SDK." }
    return $tool.FullName
}

# --- Build the exe ---------------------------------------------------------
Write-Host "Building winremap.exe ($Configuration)..." -ForegroundColor Cyan
Push-Location $repo
try {
    $cargoArgs = @('build')
    if ($Configuration -eq 'release') { $cargoArgs += '--release' }
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
}
$exe = Join-Path $repo "target\$Configuration\winremap.exe"
if (-not (Test-Path $exe)) { throw "missing $exe" }

# --- Assemble the layout ---------------------------------------------------
# Rebuilt from scratch every time: a stale file left in the layout ends up in
# the package, and a package is exactly the wrong place to discover that.
Write-Host "Assembling $layout..." -ForegroundColor Cyan
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Path $layout | Out-Null
Copy-Item $exe (Join-Path $layout 'winremap.exe')
Copy-Item (Join-Path $here 'Assets') (Join-Path $layout 'Assets') -Recurse

$manifestPath = Join-Path $layout 'AppxManifest.xml'
Copy-Item (Join-Path $here 'AppxManifest.xml') $manifestPath

# Keep the package version in step with Cargo.toml so a build can never claim
# to be a version it is not. The Store owns the fourth field, hence the 0.
$cargoVersion = (Select-String -Path (Join-Path $repo 'Cargo.toml') -Pattern '^version = "(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$xml = [xml](Get-Content $manifestPath -Raw)
$xml.Package.Identity.Version = "$cargoVersion.0"

$selfSignSubject = 'CN=WinRemap Sideload Test'
if ($SelfSign) {
    # Windows refuses a package whose Publisher differs from its signing
    # subject, and the Store publisher ID is not something we can sign as.
    $xml.Package.Identity.Publisher = $selfSignSubject
}
$xml.Save($manifestPath)
Write-Host "  version $cargoVersion.0, publisher $($xml.Package.Identity.Publisher)"

# --- Package Resource Index ------------------------------------------------
# Without this, every qualified asset name in Assets\ is dead weight.
#
# The manifest names one file (`Assets\Square44x44Logo.png`); the variants that
# carry `scale-`, `targetsize-` and `_altform-unplated` are found through the
# package's resource index, not by looking in the folder. A package with no
# resources.pri therefore uses the literal file and nothing else - which is why
# adding the five unplated PNGs on 2026-08-10 changed nothing on screen, and
# why Settings > Apps > Startup still drew the plated (blue-on-blue) form
# (owner report, 2026-08-11).
#
# The official walkthrough puts this step immediately after "add target-based
# unplated assets", in those words: "If you create target-based assets ... you'll
# have to generate a new PRI file."
# https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion
#
# priconfig.xml lives outside the layout on purpose: makepri indexes the folder
# it is given, and a config file sitting in there becomes a package resource.
Write-Host 'Indexing resources (makepri)...' -ForegroundColor Cyan
$makepri = Get-SdkTool 'makepri.exe'
$priConfig = Join-Path $here 'out\priconfig.xml'
New-Item -ItemType Directory -Force -Path (Split-Path $priConfig) | Out-Null
& $makepri createconfig /cf $priConfig /dq en-US /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makepri createconfig failed ($LASTEXITCODE)" }
& $makepri new /pr $layout /cf $priConfig /of (Join-Path $layout 'resources.pri') /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makepri new failed ($LASTEXITCODE)" }
# Read the index back and insist the unplated candidates are in it. "The file
# is in the folder" was the check that passed on 2026-08-10 while the icon was
# still wrong; this is the question that was actually being asked.
$priDump = Join-Path $here 'out\resources-dump.xml'
& $makepri dump /if (Join-Path $layout 'resources.pri') /of $priDump /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makepri dump failed ($LASTEXITCODE)" }
$unplated = ([regex]::Matches((Get-Content $priDump -Raw), 'altform-unplated')).Count
if ($unplated -eq 0) { throw 'resources.pri carries no altform-unplated candidate - the unplated assets would be ignored' }
Write-Host "  resources.pri: $((Get-Item (Join-Path $layout 'resources.pri')).Length) bytes, altform-unplated x $unplated"

# --- Register --------------------------------------------------------------
if ($Register) {
    $devMode = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' `
        -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue
    if (-not $devMode -or $devMode.AllowDevelopmentWithoutDevLicense -ne 1) {
        Write-Warning 'Developer Mode looks off. Settings > System > For developers > Developer Mode.'
    }
    Write-Host 'Registering the layout...' -ForegroundColor Cyan
    # Remove first: registering over an existing registration keeps the old
    # files when only the manifest changed.
    Get-AppxPackage -Name 'SUGANUMADaiki.WinRemap' -ErrorAction SilentlyContinue |
        Remove-AppxPackage -ErrorAction SilentlyContinue
    Add-AppxPackage -Register $manifestPath
    $pkg = Get-AppxPackage -Name 'SUGANUMADaiki.WinRemap'
    Write-Host "Registered $($pkg.PackageFullName)" -ForegroundColor Green
    Write-Host "  data: $env:LOCALAPPDATA\Packages\$($pkg.PackageFamilyName)"
    Write-Host '  launch from the Start menu, or: explorer.exe shell:AppsFolder\' -NoNewline
    Write-Host "$($pkg.PackageFamilyName)!WinRemap"
    return
}

# --- Pack ------------------------------------------------------------------
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$msix = Join-Path $OutDir "winremap-$cargoVersion.msix"
if (Test-Path $msix) { Remove-Item -Force $msix }

Write-Host "Packing $msix..." -ForegroundColor Cyan
& (Get-SdkTool 'makeappx.exe') pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }

if (-not $SelfSign) {
    Write-Host "Packed $msix" -ForegroundColor Green
    Write-Host '  unsigned on purpose: the Store signs Store packages.'
    return
}

# --- Self-sign -------------------------------------------------------------
$cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $selfSignSubject } | Select-Object -First 1
if (-not $cert) {
    Write-Host 'Creating the sideload certificate...' -ForegroundColor Cyan
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $selfSignSubject `
        -CertStoreLocation 'Cert:\CurrentUser\My' -KeyUsage DigitalSignature `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
}
& (Get-SdkTool 'signtool.exe') sign /fd SHA256 /sha1 $cert.Thumbprint $msix
if ($LASTEXITCODE -ne 0) { throw "signtool failed ($LASTEXITCODE)" }

Write-Host "Signed $msix" -ForegroundColor Green
Write-Host '  to install, trust the certificate first (administrator):'
Write-Host "    Export-Certificate -Cert Cert:\CurrentUser\My\$($cert.Thumbprint) -FilePath winremap-test.cer"
Write-Host '    Import-Certificate -FilePath winremap-test.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople'
Write-Host "    Add-AppxPackage $msix"
