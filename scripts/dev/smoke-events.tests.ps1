# Run with powershell.exe or pwsh.exe (fork-only). The equality test: scripts/smoke.ps1 and
# scripts/dev/smoke-events.py must print the same `xcl-smoke v1` block for the same synthetic run.
# A stand-in for xcl.exe prints the path of a log written under the temp dir; nothing else is touched.
$ErrorActionPreference = 'Stop'
$smoke = Join-Path $PSScriptRoot '..\smoke.ps1'
$reader = Join-Path $PSScriptRoot 'smoke-events.py'
$py = $null
foreach ($name in 'python', 'python3', 'py') {
    $cmd = Get-Command $name -ErrorAction SilentlyContinue
    if ($cmd) { $py = $cmd.Source; break }
}
if (-not $py) { throw 'no python on PATH' }

$fixture = Join-Path ([IO.Path]::GetTempPath()) ('xcl-smoke-events-' + [guid]::NewGuid())
$null = New-Item -ItemType Directory -Path $fixture
$xcl = Join-Path $fixture 'xcl.cmd'
$log = Join-Path $fixture 'xcl.log'
$report = Join-Path $fixture 'report.json'
$game = Join-Path $fixture 'SomeTitle'
$run = Join-Path $fixture 'run-20261125T180411Z'
$null = New-Item -ItemType Directory -Path $game

function Get-Smoke { ((& $smoke -Game $game -Title SomeTitle -Seconds 1 -Xcl $xcl) -join "`n") -replace "`r", '' }
function Get-Reader {
    $out = & $py $reader --title SomeTitle --seconds 1 --log $log
    if ($LASTEXITCODE -ne 0) { throw "smoke-events.py exited with $LASTEXITCODE" }
    ($out -join "`n") -replace "`r", ''
}
function Set-Json($path, $value) { $value | ConvertTo-Json -Compress | Set-Content -LiteralPath $path -Encoding utf8 }
function Write-Run($events) {
    $null = New-Item -ItemType Directory -Path $run -Force
    Set-Json (Join-Path $run 'manifest.json') @{ log = $log; compare_key = ''; switches = @{ XCL_RUN_SECONDS = '1' } }
    ($events | ForEach-Object { $_ | ConvertTo-Json -Compress }) | Set-Content -LiteralPath (Join-Path $run 'events.jsonl') -Encoding utf8
}
$failed = $false
function Assert-Equal($case) {
    $a = Get-Smoke
    $b = Get-Reader
    if ($a -ne $b) {
        $script:failed = $true
        Write-Host "MISMATCH in case ($case)`n--- smoke.ps1`n$a`n--- smoke-events.py`n$b"
    } else {
        Write-Host "ok: case ($case)"
    }
}
try {
    "@echo off`r`necho Log:    $log" | Set-Content -LiteralPath $xcl -Encoding Ascii
    Set-Json $report @{ d3d12 = @{ adapter = 'Synthetic Adapter 9000 (32187 MiB VRAM)' } }
    $logLines = @(
        'xcl: activity (2 s): 60 presents, 240 submits'
        'xcl: memory: 1234 MB committed by the process'
        'xcl: timed run over (118 frames shown, 120 PresentX calls)'
        'xcl: process ends (thread 1)'
    )
    $events = @(
        @{ t = 1.0; ev = 'activity'; presents_2s = 60; committed_mb = 1234 }
        @{ t = 2.0; ev = 'end'; reason = 'run_seconds'; frames_shown = 118; presentx = 120 }
        @{ t = 3.0; ev = 'exit'; status = 0 }
    )

    # (a) with a run folder
    $logLines | Set-Content -LiteralPath $log -Encoding utf8
    Write-Run $events
    Assert-Equal 'a: with a run folder'

    # (b) without one (a main build writes no events)
    Remove-Item -LiteralPath $run -Recurse
    Assert-Equal 'b: without a run folder'

    # (c) a device removal and a crash in the log, the crash also as an event
    $logLines + 'xcl: D3D12.X: device removed (0x887a0006)' + 'xcl: unhandled exception 0xc0000005' | Set-Content -LiteralPath $log -Encoding utf8
    Write-Run ($events + @{ t = 2.5; ev = 'error'; kind = 'title_exception'; fatal = $true })
    Assert-Equal 'c: device removal and crash, with a run folder'
    Remove-Item -LiteralPath $run -Recurse
    Assert-Equal 'c: device removal and crash, log only'
} finally {
    foreach ($path in $run, $game, $fixture) {
        if ((Test-Path -LiteralPath $path) -and $path.StartsWith([IO.Path]::GetTempPath())) {
            Remove-Item -LiteralPath $path -Recurse -Force
        }
    }
}
if ($failed) { exit 1 }
