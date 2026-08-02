# Walks an acceptance checklist one item at a time: prepares what can be
# prepared, shows what to do, waits for a person to do it on a real keyboard,
# and writes the answer back into the same document (ADR 0069).
#
#   .\tests\acceptance\run-acceptance.ps1 docs\v0.8\03_acceptance-checklist.md
#   .\tests\acceptance\run-acceptance.ps1 <checklist> -Only S-1,S-2
#   .\tests\acceptance\run-acceptance.ps1 <checklist> -Resume
#   .\tests\acceptance\run-acceptance.ps1 <checklist> -List        # parse only
#
# **No keys are injected here, ever.** Typing on the real keyboard is what the
# acceptance is for; anything a synthetic key could confirm is already covered
# by tests/ui. The harness does not use `--accept-injected` either — the
# acceptance runs against a plain shipping build (ADR 0069 decision 6).
#
# The items come out of the checklist's own tables, and the answers go back
# into the same file between the `harness:begin` / `harness:end` markers, so
# there is exactly one document and nobody transcribes anything.
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$Checklist,
    # Only these items, in this order.
    [string[]]$Only,
    # Items with no record yet, plus the ones whose latest answer was
    # "測れない" or "不合格" — the two that are meant to come back.
    [switch]$Resume,
    # Parse and print, change nothing. This is how an agent checks that the
    # tables still parse, since it cannot answer the prompts.
    [switch]$List,
    # Self-test: answers from a file, `<id>|<1-4>|<note>` per line, instead of
    # from a person. Recorded as such — see Write-Records.
    [string]$Answers,
    [switch]$DryRun,
    [string]$Exe = ".\target\release\winremap.exe",
    # Where the live configuration is. The packaged build keeps it somewhere
    # else (ADR 0061), and the §2 preparation moves this file aside.
    [string]$ConfigPath = (Join-Path $env:APPDATA 'winremap\config.toml')
)

$ErrorActionPreference = 'Stop'
$script:begin = '<!-- harness:begin -->'
$script:end = '<!-- harness:end -->'

# --- reading the checklist ---------------------------------------------------

# Splits a Markdown table row into cells. `\|` is an escaped pipe inside a
# cell, not a separator — P-5's row has one (`Get-AppxPackage ... \| Remove-`).
function Split-Row([string]$line) {
    $cells = [regex]::Split($line.Trim().Trim('|'), '(?<!\\)\|')
    return $cells | ForEach-Object { $_.Trim() -replace '\\\|', '|' }
}

# Every row of every table whose first cell is an item id in bold: `| **S-1**
# | ... |`. That shape is what all three tables (§2 §3 §4) already use, and
# it is specific enough that no prose row matches it.
#
# The columns are taken positionally but *labelled* from the table's own
# header, because the tables do not agree on what column 4 is: §4 calls it
# 通過条件, §3 calls it 「v0.7 での見どころ」. Printing the checklist's own
# word for it keeps the harness from asserting something the document does
# not say.
function Get-Items([string]$path) {
    $items = @()
    $labels = @('#', '何を見るか', '手順', '')
    foreach ($line in Get-Content $path) {
        if ($line -match '^\s*\|\s*#\s*\|') { $labels = Split-Row $line; continue }
        if ($line -notmatch '^\s*\|\s*\*\*([A-Za-z]+-\d+)\*\*\s*\|') { continue }
        $cells = Split-Row $line
        $items += [pscustomobject]@{
            Id        = $Matches[1]
            What      = $cells[1]
            How       = $cells[2]
            HowLabel  = $labels[2]
            Pass      = if ($cells.Count -gt 3) { $cells[3] } else { '' }
            PassLabel = if ($labels.Count -gt 3) { $labels[3] } else { '' }
        }
    }
    return $items
}

# Markdown emphasis is noise on a terminal, and the checklist is full of it.
function Show-Plain([string]$text) { return ($text -replace '\*\*', '') }

# The answers already in the file, oldest first. Used by -Resume and shown
# next to each item so the person can see what was said last time.
function Get-Records([string]$path) {
    $records = @()
    $inside = $false
    $stamp = ''
    foreach ($line in Get-Content $path) {
        if ($line -match [regex]::Escape($script:begin)) { $inside = $true; continue }
        if ($line -match [regex]::Escape($script:end)) { $inside = $false; continue }
        if (-not $inside) { continue }
        if ($line -match '^####\s+(\S+)') { $stamp = $Matches[1]; continue }
        if ($line -notmatch '^\s*\|\s*([A-Za-z]+-\d+)\s*\|') { continue }
        $cells = Split-Row $line
        $records += [pscustomobject]@{ Id = $Matches[1]; Date = $stamp; Verdict = $cells[1]; Note = $cells[2] }
    }
    return $records
}

# --- the run's environment ---------------------------------------------------

Add-Type -Namespace Acc -Name Kbd -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
public static extern bool GetKeyboardLayoutNameW(System.Text.StringBuilder name);
'@

# The one fact v0.7's records were missing. Asking a person to write down
# their keyboard layout was tried, and it did not get written down — so the
# harness writes it (ADR 0069 decision 4).
function Get-LayoutName {
    $buffer = New-Object System.Text.StringBuilder 9
    if (-not [Acc.Kbd]::GetKeyboardLayoutNameW($buffer)) { return 'キーボード配列: 取得できず' }
    $id = $buffer.ToString()
    $key = "HKLM:\SYSTEM\CurrentControlSet\Control\Keyboard Layouts\$id"
    $text = try { (Get-ItemProperty $key -ErrorAction Stop).'Layout Text' } catch { $null }
    return $(if ($text) { "キーボード: $text（$id）" } else { "キーボード: $id" })
}

function Get-Environment {
    $parts = @(Get-LayoutName)
    if (Test-Path $Exe) {
        # `--version` prints the crate name, which is the lower-case
        # identifier form (ADR 0025); the record is prose, so use the product
        # spelling.
        $version = (& $Exe --version 2>&1 | Select-Object -First 1)
        if ($version) { $parts += "WinRemap $($version -replace '^winremap\s+', '')" }
    }
    $os = Get-CimInstance Win32_OperatingSystem
    $parts += "$($os.Caption)（$($os.BuildNumber)）"
    return ($parts -join ' / ')
}

# --- preparing the environment -----------------------------------------------

# What can be set up safely, by item prefix. Anything destructive stays in
# `Manual`: it is printed and the person confirms, rather than run (ADR 0069
# decision 6). An unknown prefix simply gets no preparation.
$script:config = $ConfigPath
$script:backup = "$ConfigPath.bak"

$script:prep = @{
    'S' = @{
        Name  = '記号キー'
        Enter = {
            if (Test-Path $script:backup) {
                throw "$script:backup が既にある。前回の受け入れが片付いていない — 中身を確かめてから手で戻すこと"
            }
            if (Test-Path $script:config) {
                Move-Item $script:config $script:backup
                "  実運用の設定を $script:backup へ退避した"
            }
            Copy-Item (Join-Path $PSScriptRoot 'symbol-keys.toml') $script:config
            "  受け入れ用の設定を置いた（tests\acceptance\symbol-keys.toml）"
        }
        Leave = {
            if (Test-Path $script:backup) {
                Move-Item -Force $script:backup $script:config
                "  実運用の設定を戻した"
            }
        }
        Manual = @(
            'WinRemap を起動しておく（常駐していなければ）',
            'トレイ →「ログを表示」でログウィンドウを開いておく',
            'メモ帳を開いておく'
        )
    }
    'P' = @{
        Name   = 'MSIX 固有'
        Manual = @(
            'チェックリスト §4.1 の事前準備を先に済ませること',
            'パッケージの登録・削除はハーネスからは行わない（実運用の設定を壊しうるため）'
        )
    }
}

# --- asking -------------------------------------------------------------------

$script:verdicts = @{
    '1' = @{ Text = '通過'; Ask = '何をどう確かめたか' }
    '2' = @{ Text = '不合格'; Ask = '何が起きたか' }
    '3' = @{ Text = '未実施'; Ask = '省略した判断の理由' }
    '4' = @{ Text = '測れない'; Ask = '何が揃えば測れるか' }
}

$script:scripted = $null
if ($Answers) {
    $script:scripted = @{}
    foreach ($line in Get-Content $Answers) {
        if ($line.Trim() -eq '' -or $line.StartsWith('#')) { continue }
        $parts = $line -split '\|', 3
        $script:scripted[$parts[0].Trim()] = @{ Choice = $parts[1].Trim(); Note = $parts[2].Trim() }
    }
}

function Read-Answer([string]$id) {
    if ($null -ne $script:scripted) {
        if (-not $script:scripted.ContainsKey($id)) { return $null }
        $canned = $script:scripted[$id]
        return @{ Verdict = $script:verdicts[$canned.Choice].Text; Note = $canned.Note }
    }
    while ($true) {
        $choice = (Read-Host '  [1] 通過  [2] 不合格  [3] 未実施  [4] 測れない  [q] 中断').Trim()
        if ($choice -eq 'q') { return $null }
        if (-not $script:verdicts.ContainsKey($choice)) { continue }
        $verdict = $script:verdicts[$choice]
        # Required for every verdict: the checklist's own rule is "do not just
        # write OK", and the harness must not be the thing that lets an empty
        # record through (ADR 0069 decision 3).
        while ($true) {
            $note = (Read-Host "  $($verdict.Ask)").Trim()
            if ($note -ne '') { return @{ Verdict = $verdict.Text; Note = $note } }
            '  記録は省略できない。1 行でよいので書くこと'
        }
    }
}

# --- writing back -------------------------------------------------------------

function Assert-Markers([string]$path) {
    if ([IO.File]::ReadAllText($path) -match [regex]::Escape($script:begin)) { return }
    throw @"
$path に記録欄のマーカーが無い。次の 2 行を記録欄に貼ってから、もう一度実行すること:

$script:begin
$script:end
"@
}

function Write-Records([string]$path, $rows, [string]$environment) {
    $text = [IO.File]::ReadAllText($path)
    $eol = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
    $stamp = Get-Date -Format 'yyyy-MM-dd'
    $head = "#### $stamp — $environment"
    if ($null -ne $script:scripted) { $head += '（**-Answers による自己テスト。人は打っていない**）' }
    $block = @($head, '', '| 項目 | 結果 | 記録 |', '|---|---|---|')
    foreach ($row in $rows) {
        $block += "| $($row.Id) | $($row.Verdict) | $($row.Note.Replace('|', '\|')) |"
    }
    $block += ''
    $addition = ($block -join $eol) + $eol
    $text = $text.Replace($script:end, $addition + $script:end)
    [IO.File]::WriteAllText($path, $text, (New-Object Text.UTF8Encoding($false)))
}

# --- the run -------------------------------------------------------------------

if (-not (Test-Path $Checklist)) { throw "チェックリストが無い: $Checklist" }
$items = Get-Items $Checklist
if ($items.Count -eq 0) { throw "$Checklist から項目を 1 つも読めなかった（表の形が変わった？ -List で確かめること）" }
$records = Get-Records $Checklist

if ($Only) {
    $wanted = $Only | ForEach-Object { $_.ToUpper() }
    $items = $items | Where-Object { $wanted -contains $_.Id.ToUpper() }
}
if ($Resume) {
    $items = $items | Where-Object {
        $last = $records | Where-Object Id -eq $_.Id | Select-Object -Last 1
        # No answer yet, or an answer that was meant to come back.
        (-not $last) -or ($last.Verdict -in '測れない', '不合格')
    }
}

if ($List) {
    "$Checklist から $($items.Count) 項目"
    foreach ($item in $items) {
        $last = $records | Where-Object Id -eq $item.Id | Select-Object -Last 1
        "{0,-6} {1}{2}" -f $item.Id, (Show-Plain $item.What), $(if ($last) { "  [前回: $($last.Verdict)]" } else { '' })
    }
    exit 0
}

if ($items.Count -eq 0) { "やることは残っていない。"; exit 0 }
# Before anything is asked, not at write time: finding out that the answers
# have nowhere to go *after* somebody has worked through the list is the one
# failure this must not have.
if (-not $DryRun) { Assert-Markers $Checklist }

$environment = Get-Environment
''
"受け入れ: $Checklist"
"環境:     $environment"
"項目:     $($items.Count) 件"
'キーは一切注入しない。実キーボードで打つこと。'
''

$answered = @()
$entered = @{}
try {
    foreach ($item in $items) {
        $prefix = $item.Id.Split('-')[0]
        if ($script:prep.ContainsKey($prefix) -and -not $entered.ContainsKey($prefix)) {
            $section = $script:prep[$prefix]
            ''
            "=== $($section.Name) の準備 ==="
            if ($section.Enter -and -not $DryRun) { & $section.Enter }
            # Only now: the teardown must not run for a preparation that
            # failed. `Enter` refuses when a stale backup is already there,
            # and a teardown running anyway would restore that stale file
            # over the live config.
            $entered[$prefix] = $true
            foreach ($step in $section.Manual) { "  [人] $step" }
            if ($null -eq $script:scripted) { [void](Read-Host '  済んだら Enter') }
        }

        ''
        '─────────────────────────────────────────────'
        "$($item.Id)  $(Show-Plain $item.What)"
        ''
        "  $($item.HowLabel): $(Show-Plain $item.How)"
        if ($item.Pass) { "  $($item.PassLabel): $(Show-Plain $item.Pass)" }
        $last = $records | Where-Object Id -eq $item.Id | Select-Object -Last 1
        if ($last) { "  前回（$($last.Date)）: $($last.Verdict) — $($last.Note)" }
        ''

        $answer = Read-Answer $item.Id
        if ($null -eq $answer) {
            if ($null -eq $script:scripted) { '  中断した。ここまでの分を記録する'; break }
            continue
        }
        $answered += [pscustomobject]@{ Id = $item.Id; Verdict = $answer.Verdict; Note = $answer.Note }
    }
}
finally {
    foreach ($prefix in $entered.Keys) {
        $section = $script:prep[$prefix]
        if ($section.Leave -and -not $DryRun) {
            ''
            "=== $($section.Name) の後片付け ==="
            & $section.Leave
        }
    }
}

''
if ($answered.Count -eq 0) { '記録するものが無い。'; exit 0 }
foreach ($row in $answered) { "{0,-6} {1}" -f $row.Id, $row.Verdict }
''
if ($DryRun) {
    "-DryRun のため書き戻さない（$($answered.Count) 件）"
} else {
    Write-Records $Checklist $answered $environment
    "$Checklist の記録欄に $($answered.Count) 件を追記した"
}
