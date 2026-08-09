<#
.SYNOPSIS
    Builds the help site from site-src/ into site/ (ADR 0066).

.DESCRIPTION
    Two steps, and the second one is the reason this script exists:

      1. `zola build` into a temporary directory.
      2. Flatten `install.html/index.html` into `install.html`, and copy the
         result over site/.

    Zola renders a page whose `path` is `install.html` as a *directory* called
    `install.html` holding an `index.html`. The site has been published at
    `.../install.html` since v0.1 and those URLs are in the README, the
    release notes, the Store listing and whatever Google has indexed, so the
    output is flattened rather than the URLs changed.

    site/ is generated. Do not edit it; edit site-src/ and run this.

.EXAMPLE
    .\site-src\build.ps1
    .\site-src\build.ps1 -Check   # build and report whether site/ is stale
#>
[CmdletBinding()]
param(
    # Leaves site/ untouched and exits non-zero if the build would change it.
    # This is what CI runs, so a forgotten `build.ps1` cannot ship.
    [switch]$Check
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Resolve-Path (Join-Path $here '..')
$site = Join-Path $repo 'site'
$staging = Join-Path ([IO.Path]::GetTempPath()) ("winremap-site-" + [Guid]::NewGuid().ToString('N'))

if (-not (Get-Command zola -ErrorAction SilentlyContinue)) {
    throw 'zola not found. Install it (scoop install zola) and try again.'
}

& zola --root $here build --output-dir $staging --force
if ($LASTEXITCODE -ne 0) { throw "zola build failed ($LASTEXITCODE)" }

# Zola writes a bare `<h1>404 Not Found</h1>` unless a template says otherwise.
# The hand-written site had no 404 page, and publishing an unstyled one would
# replace GitHub's — which is a page to design, not to inherit by accident.
$notFound = Join-Path $staging '404.html'
if (Test-Path $notFound) { Remove-Item $notFound }

# `page.html/index.html` -> `page.html`. Deepest first, so a nested directory
# is flattened before its parent is walked over.
Get-ChildItem $staging -Recurse -Directory -Filter '*.html' |
    Sort-Object { $_.FullName.Length } -Descending |
    ForEach-Object {
        $index = Join-Path $_.FullName 'index.html'
        if (-not (Test-Path $index)) { return }
        $target = $_.FullName
        Move-Item $index "$target.tmp"
        Remove-Item $target -Recurse -Force
        Move-Item "$target.tmp" $target
    }

if ($Check) {
    # Compare content, not timestamps: the question is whether what is
    # committed matches what site-src/ produces.
    $diff = @()
    $built = Get-ChildItem $staging -Recurse -File
    foreach ($f in $built) {
        $rel = $f.FullName.Substring($staging.Length + 1)
        $existing = Join-Path $site $rel
        if (-not (Test-Path $existing)) { $diff += "missing in site/: $rel"; continue }
        if ((Get-FileHash $f.FullName).Hash -ne (Get-FileHash $existing).Hash) { $diff += "differs: $rel" }
    }
    foreach ($f in Get-ChildItem $site -Recurse -File) {
        $rel = $f.FullName.Substring($site.Length + 1)
        if (-not (Test-Path (Join-Path $staging $rel))) { $diff += "not produced by the build: $rel" }
    }
    Remove-Item $staging -Recurse -Force
    if ($diff) {
        $diff | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
        throw "site/ is out of date with site-src/ ($($diff.Count) file(s)). Run site-src\build.ps1 and commit."
    }
    Write-Host "site/ matches site-src/ ($($built.Count) files)." -ForegroundColor Green
    return
}

if (Test-Path $site) { Remove-Item $site -Recurse -Force }
Move-Item $staging $site
Write-Host "built $((Get-ChildItem $site -Recurse -File).Count) files into $site" -ForegroundColor Cyan
