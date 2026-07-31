# Dot-sourced by the guest-side checks that drive the UI through the Windows
# App Development CLI (`winapp`) rather than through a plain UIA client.
#
# Extracted from probe-winapp.ps1, which measured the gate in
# docs/v0.5/notes/20260727_winapp-cli-migration.md section 3. Every rule below
# is one the probe learned the hard way; the comments say which failure each
# one prevents, because none of them are guessable from the tool's help text.
#
# The caller provides Say([string]) before dot-sourcing this.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932,
# so a Japanese literal here would arrive mangled.

$env:Path = $env:Path + ';' + $env:LOCALAPPDATA + '\Microsoft\WindowsApps'

# One [string[]] parameter, not ValueFromRemainingArguments: an array handed to
# a remaining-arguments parameter collapses into a single string, and winapp
# then sees one long argument and prints its usage.
function W {
    param([string[]]$A)
    $text = (& winapp @A 2>&1 | Out-String)
    return [pscustomobject]@{ Code = $LASTEXITCODE; Text = $text.TrimEnd() }
}

# Always read the JSON, never the text output: text comes back as mojibake
# through PowerShell 5.1 (UTF-8 decoded as CP932), while \uXXXX in the JSON
# survives ConvertFrom-Json intact - and this guest's Windows is Japanese.
function WJson {
    param([string[]]$A)
    $text = (& winapp @A 2>$null | Out-String)
    if ($LASTEXITCODE -ne 0 -or -not $text.Trim()) { return @() }
    # Assigned to a variable first on purpose: `return @(pipeline)` keeps the
    # array as one item, because ConvertFrom-Json emits a JSON array as a single
    # object and @() only wraps what it was handed. Through a variable the array
    # enumerates, and the caller gets one object per window.
    try { $obj = $text | ConvertFrom-Json } catch { return @() }
    return @($obj)
}

function Flatten($node, [System.Collections.Generic.List[object]]$acc) {
    if ($null -eq $node) { return }
    foreach ($n in @($node)) {
        if ($null -eq $n) { continue }
        $acc.Add($n)
        if ($n.PSObject.Properties.Name -contains "children") { Flatten $n.children $acc }
    }
}

# Every element of a window, flattened. `inspect --json` nests as
# { windows: [ { elements: [ { ..., children: [...] } ] } ] }.
function WinappElements([string[]]$Target) {
    $json = WJson (@("ui", "inspect", "--json", "-d", "20") + $Target)
    $acc = New-Object System.Collections.Generic.List[object]
    foreach ($w in @($json)) {
        if ($w.PSObject.Properties.Name -contains "windows") {
            foreach ($win in @($w.windows)) { Flatten $win.elements $acc }
        }
    }
    return $acc
}

function WinappName($e) {
    if ($e.PSObject.Properties.Name -contains "name" -and $e.name) { return [string]$e.name }
    return ""
}
function WinappType($e) {
    if ($e.PSObject.Properties.Name -contains "type" -and $e.type) { return [string]$e.type }
    return ""
}

# Elements whose name matches, optionally narrowed by control type. Returns
# them all rather than the first: "how many carry this name" is the question
# behind the ambiguity trap below, and a caller that wants one can say so.
#
# `return ,$hits` — the leading comma is not a typo and is not optional.
# PowerShell unwraps a one-element array on the way out of a function, so a
# search that matched exactly one element would hand back the element itself,
# and `.Count` on a PSCustomObject is **empty** in PowerShell 5.1 (5.1 does not
# give every object the Count member that later versions do). Every assertion
# of the form `$hits.Count -eq 1` then reads as false while the element is
# sitting right there. That cost the first run of this check four false
# failures — and it failed in the shape of "the settings window is missing its
# Edit button", which is exactly the kind of lie the migration is meant to end.
# The comma wraps the array in an outer one; PowerShell unwraps that instead.
function WinappFind($els, [string]$Name, [string]$Type = "") {
    $hits = @()
    foreach ($e in $els) {
        if ((WinappName $e) -ne $Name) { continue }
        if ($Type -and (WinappType $e) -notmatch $Type) { continue }
        $hits += $e
    }
    return , $hits
}

function WinappFindLike($els, [string]$Pattern, [string]$Type = "") {
    $hits = @()
    foreach ($e in $els) {
        if ((WinappName $e) -notlike $Pattern) { continue }
        if ($Type -and (WinappType $e) -notmatch $Type) { continue }
        $hits += $e
    }
    return , $hits
}

# Every window of a process, as winapp sees them.
function WinappWindows([string]$App) {
    return , @(WJson @("ui", "list-windows", "--app", $App, "--json"))
}

function WinappWindow([string]$App, [string]$TitleLike) {
    foreach ($w in (WinappWindows $App)) {
        if ($w.PSObject.Properties.Name -contains "title" -and $w.title -like $TitleLike) { return $w }
    }
    return $null
}

# `wait-for` reports a name matching two elements as NOT FOUND rather than as
# ambiguous, which is how a working assertion looked like a failing one for two
# runs of the probe ("Save" is both a button and a word in the status line).
# So: resolve the name to a run-specific slug through `search` first, and wait
# on that. The slug changes every run, which is why it is never hard-coded.
function WinappSlug([string]$Name, [string[]]$Target, [string]$Type = "") {
    $hit = WJson (@("ui", "search", $Name, "--json") + $Target)
    foreach ($h in @($hit)) {
        if ($h.PSObject.Properties.Name -notcontains "matches") { continue }
        foreach ($m in @($h.matches)) {
            if ((WinappName $m) -ne $Name) { continue }
            if ($Type -and (WinappType $m) -notmatch $Type) { continue }
            return [string]$m.selector
        }
    }
    return $null
}

# Waits for a transition instead of sleeping through it. `--timeout` is in
# MILLISECONDS (the default is 5000); seconds here would wait 5 ms and call the
# element missing. wait-for does not wait for the app to start either - with no
# process it fails at once - so process startup is Get-Process's job.
function WinappWaitFor([string]$Name, [string[]]$Target, [int]$TimeoutMs = 5000) {
    return (W (@("ui", "wait-for", $Name, "--timeout", "$TimeoutMs") + $Target)).Code -eq 0
}
