# Tools for walking an acceptance checklist with a person (ADR 0070).
#
# **This script does not ask anything.** The dialogue happens in the AI
# agent's prompt — the agent reads an item out, the person does it on a real
# keyboard and says what happened in their own words, and the agent calls
# `-Record` here. What lives in this file is only what a conversation cannot
# do: parse the checklist, stamp the environment, move the live configuration
# aside and back, and append to the document.
#
#   run-acceptance.ps1 <checklist>                    # 一覧（既定）
#   run-acceptance.ps1 <checklist> -Resume            # 未記録＋測れない／不合格
#   run-acceptance.ps1 <checklist> -Show C-1,M-2      # 手順と通過条件
#   run-acceptance.ps1 <checklist> -Prepare H         # 設定の配置と起動
#   run-acceptance.ps1 -Teardown                      # 実運用の設定を戻す
#   run-acceptance.ps1 <checklist> -Record ans.txt    # 記録欄へ追記
#   run-acceptance.ps1 -Environment                   # 環境の 1 行
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
    # Not mandatory: -Teardown and -Environment do not need it, and teardown
    # has to work when a run went wrong and the live configuration is still
    # moved aside.
    [Parameter(Position = 0)]
    [string]$Checklist,

    # --- what to do (pick one; the default is the listing) ---
    # Print id, summary and the latest verdict, one line each.
    [switch]$List,
    # Print the full 手順 / 通過条件 / 前回 of these items — what the agent
    # reads out to the person.
    [string[]]$Show,
    # Put the acceptance configuration in place for an item prefix
    # (C/F/H/M/S/P)
    # and print the steps that stay with the person.
    [string]$Prepare,
    # Put the live configuration back. Safe to run at any time, including
    # after a session that died halfway.
    [switch]$Teardown,
    # Append answers to the record section. The file has one line per item:
    # `<id>|<通過|不合格|未実施|測れない or 1-4>|<note>`
    [string]$Record,
    # Mark the appended block as a self-test — answers that no person typed.
    # See Write-Records: the record must never be able to claim otherwise.
    [switch]$SelfTest,
    # Print the environment line that -Record would stamp.
    [switch]$Environment,

    # --- filters for -List ---
    [string[]]$Only,
    # Items with no record yet, plus the ones whose latest answer was
    # "測れない" or "不合格" — the two that are meant to come back.
    [switch]$Resume,

    [string]$Exe = ".\target\release\winremap.exe",
    # Where the live configuration is. The packaged build keeps it somewhere
    # else (ADR 0061), and -Prepare moves this file aside.
    [string]$ConfigPath = (Join-Path $env:APPDATA 'winremap\config.toml')
)

$ErrorActionPreference = 'Stop'
$script:begin = '<!-- harness:begin -->'
$script:end = '<!-- harness:end -->'
$script:config = $ConfigPath
$script:backup = "$ConfigPath.bak"
# The files `-Prepare F` puts *beside* the config, so there is something to
# switch to (ADR 0077) and something that fails to load (ADR 0079). Removed by
# -Teardown.
$script:extras = @('file-switch.toml', 'broken.toml')
# Which config file to open next time (ADR 0077). Not a config, but the F
# items rewrite it, so it is moved aside like one.
$script:memory = Join-Path (Split-Path $ConfigPath) 'last-config.txt'
$script:memoryBackup = "$script:memory.bak"

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

# `[IO.File]` reads the *process* current directory, which is where PowerShell
# was opened, not where `Set-Location` has since gone. Resolving once here
# keeps a relative path from passing Test-Path and then failing on write-back.
function Resolve-Checklist {
    if (-not $Checklist) { throw 'チェックリストのパスが要る' }
    if (-not (Test-Path $Checklist)) { throw "チェックリストが無い: $Checklist" }
    return (Resolve-Path -LiteralPath $Checklist).ProviderPath
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

# Puts an acceptance configuration in place, keeping the live one. Refuses
# rather than overwrite: a stale backup means a previous run did not finish,
# and the file under it is the owner's real configuration.
function Use-Config([string]$fixture) {
    if (Test-Path $script:backup) {
        throw "$script:backup が既にある。前回の受け入れが片付いていない — 中身を確かめてから -Teardown で戻すこと"
    }
    if (Test-Path $script:config) {
        Move-Item $script:config $script:backup
        "  実運用の設定を $script:backup へ退避した"
    }
    else {
        New-Item -ItemType Directory -Force -Path (Split-Path $script:config) | Out-Null
    }
    Copy-Item (Join-Path $PSScriptRoot $fixture) $script:config
    "  受け入れ用の設定を置いた（tests\acceptance\$fixture）"
}

function Restore-Config {
    if (Test-Path $script:backup) {
        Move-Item -Force $script:backup $script:config
        "  実運用の設定を戻した（$script:config）"
    }
    else {
        '  退避された設定は無い（戻すものが無い）'
    }
    Restore-Extras
}

# The second `.toml` to switch to, the one that will not load, and the note of
# which file to open next time (F-1〜F-4, ADR 0077/0079).
function Use-Extras {
    $folder = Split-Path $script:config
    foreach ($name in $script:extras) {
        Copy-Item (Join-Path $PSScriptRoot $name) (Join-Path $folder $name) -Force
    }
    "  切り替え先の設定を置いた（$($script:extras -join '・')）"
    if (Test-Path $script:memoryBackup) {
        throw "$script:memoryBackup が既にある。前回の受け入れが片付いていない — 中身を確かめてから -Teardown で戻すこと"
    }
    if (Test-Path $script:memory) {
        Move-Item $script:memory $script:memoryBackup
        "  実運用の last-config.txt を退避した（この項目群が書き換えるため）"
    }
}

function Restore-Extras {
    $folder = Split-Path $script:config
    foreach ($name in $script:extras) {
        $path = Join-Path $folder $name
        if (Test-Path $path) {
            Remove-Item $path -Force
            "  切り替え用の $name を片付けた"
        }
    }
    # The run's own choice goes, whatever it ended up being: leaving it would
    # make the owner's next plain start open a fixture.
    if (Test-Path $script:memory) {
        Remove-Item $script:memory -Force
        "  受け入れ中に書かれた last-config.txt を消した"
    }
    if (Test-Path $script:memoryBackup) {
        Move-Item -Force $script:memoryBackup $script:memory
        "  実運用の last-config.txt を戻した"
    }
}

# Only ever touches instances started from $Exe. The owner's installed or
# packaged WinRemap is somebody else's process, and the harness has no
# business killing it — it says so and lets the person decide.
function Get-HarnessProcess {
    if (-not (Test-Path $Exe)) { return @() }
    $full = (Resolve-Path -LiteralPath $Exe).ProviderPath
    return @(Get-Process winremap -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $full })
}

function Stop-HarnessProcess {
    $running = Get-HarnessProcess
    $stopped = @()
    if ($running.Count -gt 0) {
        $stopped = @($running | ForEach-Object { $_.Id })
        $running | Stop-Process -Force
        "  $Exe の常駐を終了した（$($running.Count) 個）"
    }
    # Two guards, because Stop-Process returns before the process is gone:
    # the ids we just killed, and an empty Path — which is what a dying
    # process answers with. Without them the teardown reported "ほかの
    # WinRemap が常駐している（）", an empty path, right after killing the
    # only instance there was (2026-08-04).
    $others = @(Get-Process winremap -ErrorAction SilentlyContinue |
        Where-Object { $stopped -notcontains $_.Id -and $_.Path })
    if ($others.Count -gt 0) {
        "  [人] ほかの WinRemap が常駐している（$($others[0].Path)）— 受け入れ対象はこれではない。終了しておくこと"
    }
}

# Restart rather than reload: reloading is a tray click, and the point of the
# restart is that the acceptance configuration is what the process is holding.
function Start-HarnessProcess {
    if (-not (Test-Path $Exe)) { throw "$Exe が無い。`cargo build --release` を先に" }
    Stop-HarnessProcess
    $path = (Resolve-Path -LiteralPath $Exe).ProviderPath
    Start-Process -FilePath $path
    # The path of what is *running*, not of what we asked to run. Three
    # separate misreadings in v0.8 came from observing the wrong binary: H-1
    # against a config that never reached the app (08-08), the M-2 A〜D split
    # against a WinRemap nobody had started (08-09), and P-1 against the
    # GitHub build because the Start menu shows two identically named entries
    # (08-10). Saying it out loud is what lets the person catch it.
    $started = @()
    foreach ($attempt in 1..20) {
        $started = Get-HarnessProcess
        if ($started.Count -gt 0) { break }
        Start-Sleep -Milliseconds 100
    }
    # Seeing it once is not enough. When another WinRemap already holds the
    # single-instance mutex the one just launched exits by itself, and it stays
    # in the process list for about half a second on the way out — long enough
    # to report success for something already dying (measured 2026-08-10, with
    # a packaged WinRemap resident). So look again after it has had time to go.
    if ($started.Count -gt 0) {
        Start-Sleep -Milliseconds 1000
        $started = Get-HarnessProcess
    }
    if ($started.Count -gt 0) {
        "  受け入れ用の設定で起動した: $($started[0].Path)"
        return
    }
    # Most likely another WinRemap holds the single-instance mutex, so the one
    # just launched exited by itself. Whatever the cause, the person is about
    # to be asked to look at something that is not running.
    "  [人] 起動したはずの $path が動いていない — 受け入れを始めないこと"
    foreach ($other in @(Get-Process winremap -ErrorAction SilentlyContinue | Where-Object Path)) {
        "  [人] いま動いているのは $($other.Path)"
    }
}

# What can be set up safely, by item prefix. Anything destructive stays in
# `Manual`: it is printed and the person confirms, rather than run (ADR 0069
# decision 6). An unknown prefix simply gets no preparation.
$script:prep = @{
    'C' = @{
        Name   = '--debug コンソール'
        # No Enter: C-* is the person launching `--debug` from their own
        # terminal, and a resident instance would make the second one exit.
        Enter  = { Stop-HarnessProcess }
        Manual = @(
            'ターミナル（Windows Terminal / PowerShell / cmd のどれか）を開いておく',
            'どのターミナルで見たかを報告に入れること（重なりの原因は相手側の再描画だった）'
        )
    }
    'H' = @{
        Name   = '手動最小集合'
        Enter  = { Use-Config 'manual-minimum.toml'; Start-HarnessProcess }
        Manual = @(
            'H-1 を測るターミナル（PHPStorm の内蔵ターミナル・Zed・Windows Terminal など）を開いておく',
            'トレイ →「ログを表示」でログウィンドウを開いておく — C-h が 1 文字消えるだけでは判定できない。シェル側の Emacs キーバインドでも同じことが起きる（2026-08-10 の P-7）。「[判定] C-h … → Back … に置換」の行が出ることと、C-a で行頭へ動くことを見る',
            'H-6（インストーラー経路）だけはこの準備の対象外 — 測るのはインストール済みの winremap.exe であって、この準備が起動する target\release のものではない',
            'H-3（見た目）はライトとダークの両方で見る。テーマの切り替えは Windows の設定側で行う'
        )
    }
    'M' = @{
        Name   = 'IME カーソル'
        Enter  = { Use-Config 'ime-cursor.toml'; Start-HarnessProcess }
        Manual = @(
            'メモ帳と、暗い背景のアプリ（Zed など）を開いておく — M-2 は背景の明るさで答えが変わる'
        )
    }
    'S' = @{
        Name   = '記号キー'
        Enter  = { Use-Config 'symbol-keys.toml'; Start-HarnessProcess }
        Manual = @(
            'トレイ →「ログを表示」でログウィンドウを開いておく',
            'メモ帳を開いておく'
        )
    }
    'F' = @{
        Name   = '設定ファイルの選択と表示'
        # Two files in the folder, because every F item is about *which* of
        # them is in force. manual-minimum.toml has one keymap and
        # file-switch.toml has two, so the count answers that question from
        # the tray tooltip alone.
        Enter  = { Use-Config 'manual-minimum.toml'; Use-Extras; Start-HarnessProcess }
        Manual = @(
            'トレイ →「ログを表示」でログウィンドウを開いておく — F-1・F-3 は「どのファイルを開いたか」をログの行で見る',
            'F-5（パスの省略）は設定ウィンドウの幅を変えて見る。この準備が使う %APPDATA%\winremap は短いので、省略が起きるところまで窓を狭くすること',
            'F-2 は引数付きの起動を 1 回挟む — ハーネスは引数なしでしか起動しない'
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

# --- writing back -------------------------------------------------------------

function Assert-Markers([string]$path) {
    if ([IO.File]::ReadAllText($path) -match [regex]::Escape($script:begin)) { return }
    throw @"
$path に記録欄のマーカーが無い。次の 2 行を記録欄に貼ってから、もう一度実行すること:

$script:begin
$script:end
"@
}

$script:verdicts = @{
    '1' = '通過'; '2' = '不合格'; '3' = '未実施'; '4' = '測れない'
}

function Read-AnswerFile([string]$path) {
    if (-not (Test-Path $path)) { throw "回答ファイルが無い: $path" }
    $rows = @()
    foreach ($line in Get-Content $path) {
        if ($line.Trim() -eq '' -or $line.StartsWith('#')) { continue }
        $parts = $line -split '\|', 3
        if ($parts.Count -lt 3) { throw "回答の行が `<id>|<結果>|<記録>` になっていない: $line" }
        $id = $parts[0].Trim()
        $verdict = $parts[1].Trim()
        if ($script:verdicts.ContainsKey($verdict)) { $verdict = $script:verdicts[$verdict] }
        if ($verdict -notin $script:verdicts.Values) {
            throw "$id の結果が 通過 / 不合格 / 未実施 / 測れない のどれでもない: $verdict"
        }
        $note = $parts[2].Trim()
        # The checklist's own rule is "do not just write OK", and the harness
        # must not be the thing that lets an empty record through (ADR 0069
        # decision 3).
        if ($note -eq '') { throw "$id の記録が空。1 行でよいので書くこと（測れないなら何が揃えば測れるか）" }
        $rows += [pscustomobject]@{ Id = $id; Verdict = $verdict; Note = $note }
    }
    if ($rows.Count -eq 0) { throw "$path に記録する行が無い" }
    return $rows
}

function Write-Records([string]$path, $rows, [string]$environment) {
    $text = [IO.File]::ReadAllText($path)
    $eol = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
    $stamp = Get-Date -Format 'yyyy-MM-dd'
    $head = "#### $stamp — $environment"
    # Who answered is not something the record may be vague about. The
    # default sentence is a claim about a person having typed on a real
    # keyboard; -SelfTest is the only way to append without one.
    $head += if ($SelfTest) {
        '（**自己テスト。人は打っていない**）'
    }
    else {
        '（対話: 人が実キーボードで確認し、AI エージェントが記録）'
    }
    $block = @($head, '', '| 項目 | 結果 | 記録 |', '|---|---|---|')
    foreach ($row in $rows) {
        $block += "| $($row.Id) | $($row.Verdict) | $($row.Note.Replace('|', '\|')) |"
    }
    $block += ''
    $addition = ($block -join $eol) + $eol
    $text = $text.Replace($script:end, $addition + $script:end)
    [IO.File]::WriteAllText($path, $text, (New-Object Text.UTF8Encoding($false)))
}

# --- dispatch -------------------------------------------------------------------

# Said by every command: a leftover backup means the owner's real
# configuration is not where WinRemap looks for it, and that is worth saying
# out loud whatever the person came here to do.
if (Test-Path $script:backup) {
    "※ $script:backup がある — 実運用の設定は退避されたまま。終わったら -Teardown で戻すこと"
    ''
}

if ($Environment) { Get-Environment; exit 0 }

if ($Teardown) {
    '=== 後片付け ==='
    Restore-Config
    Stop-HarnessProcess
    '  実運用の WinRemap を使っている場合は、起動し直すこと'
    exit 0
}

if ($Prepare) {
    $prefix = $Prepare.Split('-')[0].ToUpper()
    if (-not $script:prep.ContainsKey($prefix)) {
        throw "$prefix の準備は用意されていない（C / F / H / M / S / P）"
    }
    $section = $script:prep[$prefix]
    "=== $($section.Name) の準備 ==="
    if ($section.Enter) { & $section.Enter }
    foreach ($step in $section.Manual) { "  [人] $step" }
    exit 0
}

$path = Resolve-Checklist
$items = Get-Items $path
if ($items.Count -eq 0) { throw "$path から項目を 1 つも読めなかった（表の形が変わった？）" }
$records = Get-Records $path

if ($Record) {
    Assert-Markers $path
    $rows = Read-AnswerFile $Record
    $known = $items.Id
    foreach ($row in $rows) {
        if ($known -notcontains $row.Id) { throw "チェックリストに無い項目: $($row.Id)" }
    }
    # Not `$environment`: PowerShell variable names are case-insensitive, so
    # that would assign a string to the `-Environment` switch parameter.
    $envLine = Get-Environment
    Write-Records $path $rows $envLine
    foreach ($row in $rows) { "{0,-6} {1}" -f $row.Id, $row.Verdict }
    ''
    "$path の記録欄に $($rows.Count) 件を追記した（$envLine）"
    exit 0
}

if ($Show) {
    $wanted = $Show | ForEach-Object { $_.ToUpper() }
    foreach ($item in ($items | Where-Object { $wanted -contains $_.Id.ToUpper() })) {
        ''
        '─────────────────────────────────────────────'
        "$($item.Id)  $(Show-Plain $item.What)"
        ''
        "  $($item.HowLabel): $(Show-Plain $item.How)"
        if ($item.Pass) { "  $($item.PassLabel): $(Show-Plain $item.Pass)" }
        foreach ($past in ($records | Where-Object Id -eq $item.Id)) {
            "  記録（$($past.Date)）: $($past.Verdict) — $($past.Note)"
        }
    }
    ''
    exit 0
}

# The listing, and the default.
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
"$path から $($items.Count) 項目"
foreach ($item in $items) {
    $last = $records | Where-Object Id -eq $item.Id | Select-Object -Last 1
    "{0,-6} {1}{2}" -f $item.Id, (Show-Plain $item.What), $(if ($last) { "  [前回: $($last.Verdict)]" } else { '' })
}
