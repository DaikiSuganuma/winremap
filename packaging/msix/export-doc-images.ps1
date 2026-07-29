<#
.SYNOPSIS
    Trims the Store screenshots down to the window and copies them into the
    site assets, where the README and the help site can both link to them.

.DESCRIPTION
    capture-screenshots.ps1 composites each window onto a blank 1920x1080
    canvas because Partner Center wants at least 1366x768. That margin is
    wrong for a README, where the image is scaled to the column width and
    every empty pixel makes the window smaller.

    This script takes those same PNGs and cuts the margin back to a thin
    border, so one capture serves both purposes and the two can never drift
    apart. Output goes to site/assets/screenshots/ because GitHub Pages
    publishes site/, and a README image path is resolved against the
    repository -- so a single copy covers both.

    Run it after capture-screenshots.ps1. The inputs are gitignored; the
    outputs are committed.
#>
[CmdletBinding()]
param(
    # Which captures to export. The Store gets all of them; the docs only
    # need the two that carry meaning on their own.
    [string[]]$Names = @('01-settings', '03-log'),
    [string[]]$Languages = @('en', 'ja'),
    # Border left around the window, in source pixels.
    [int]$Margin = 8
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$src = Join-Path $PSScriptRoot 'screenshots'
$dst = Join-Path $root 'site\assets\screenshots'

if (-not (Test-Path $src)) {
    throw "no captures in $src -- run capture-screenshots.ps1 first"
}
if (-not (Test-Path $dst)) { New-Item -ItemType Directory -Path $dst | Out-Null }

# The canvas is a single flat colour, so the window is everything that is not
# that colour. Sampling (0,0) rather than hard-coding it keeps this working if
# the capture script's canvas colour ever changes.
function Get-ContentBounds([System.Drawing.Bitmap]$Bitmap) {
    $background = $Bitmap.GetPixel(0, 0).ToArgb()
    $left = $Bitmap.Width; $top = $Bitmap.Height; $right = -1; $bottom = -1

    # LockBits: GetPixel over two million pixels takes minutes, this takes ms.
    $rect = New-Object System.Drawing.Rectangle 0, 0, $Bitmap.Width, $Bitmap.Height
    $data = $Bitmap.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $stride = $data.Stride
        $row = New-Object int[] ($stride / 4)
        for ($y = 0; $y -lt $Bitmap.Height; $y++) {
            $scan = [IntPtr]::Add($data.Scan0, $y * $stride)
            [System.Runtime.InteropServices.Marshal]::Copy($scan, $row, 0, $row.Length)
            for ($x = 0; $x -lt $Bitmap.Width; $x++) {
                if ($row[$x] -ne $background) {
                    if ($x -lt $left) { $left = $x }
                    if ($x -gt $right) { $right = $x }
                    if ($y -lt $top) { $top = $y }
                    $bottom = $y
                }
            }
        }
    } finally {
        $Bitmap.UnlockBits($data)
    }

    if ($right -lt 0) { throw 'the image is entirely background' }
    New-Object System.Drawing.Rectangle $left, $top, ($right - $left + 1), ($bottom - $top + 1)
}

foreach ($lang in $Languages) {
    foreach ($name in $Names) {
        $file = "$lang-$name.png"
        $path = Join-Path $src $file
        if (-not (Test-Path $path)) { throw "missing capture: $path" }

        $bitmap = [System.Drawing.Bitmap]::FromFile($path)
        try {
            $bounds = Get-ContentBounds $bitmap
            $bounds.Inflate($Margin, $Margin)
            $bounds.Intersect((New-Object System.Drawing.Rectangle 0, 0, $bitmap.Width, $bitmap.Height))

            $out = $bitmap.Clone($bounds, $bitmap.PixelFormat)
            try {
                $target = Join-Path $dst $file
                $out.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
                '{0}  {1}x{2} -> {3}x{4}' -f $file, $bitmap.Width, $bitmap.Height, $out.Width, $out.Height
            } finally { $out.Dispose() }
        } finally { $bitmap.Dispose() }
    }
}
