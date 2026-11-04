param([string]$Binary = "$PSScriptRoot/../../target/release/backleaf.exe", [string]$Output = "$PSScriptRoot/../../.local/native-performance", [int]$Seconds = 10)
$ErrorActionPreference = 'Stop'
$binaryPath = (Resolve-Path -LiteralPath $Binary).Path
New-Item -ItemType Directory -Path $Output -Force | Out-Null
$outputPath = (Resolve-Path -LiteralPath $Output).Path
$cases = @(
    @{Name='1k-scroll'; Count=1000; Flow=0; Scroll=$true},
    @{Name='100k-scroll'; Count=100000; Flow=0; Scroll=$true},
    @{Name='1m-scroll'; Count=1000000; Flow=0; Scroll=$true},
    @{Name='10m-scroll'; Count=10000000; Flow=0; Scroll=$true},
    @{Name='1m-flow1'; Count=1000000; Flow=1},
    @{Name='1m-flow8'; Count=1000000; Flow=8},
    @{Name='1m-scroll-flow8'; Count=1000000; Flow=8; Scroll=$true},
    @{Name='1m-reduced'; Count=1000000; Flow=8; Reduced=$true},
    @{Name='1m-idle'; Count=1000000; Flow=0},
    @{Name='1m-wide'; Count=1000000; Flow=8; Scroll=$true; Width=2200; Height=1200},
    @{Name='1m-narrow'; Count=1000000; Flow=8; Scroll=$true; Width=560; Height=440}
)
foreach ($case in $cases) {
    $sessionPath=Join-Path $outputPath 'session-preflight.json'
    Start-Process -FilePath $binaryPath -ArgumentList @('--session-status-file',('"'+$sessionPath+'"')) -Wait -WindowStyle Hidden
    $session=Get-Content -LiteralPath $sessionPath -Raw | ConvertFrom-Json
    if ($session.locked -or $session.error) {throw 'Unlock the Windows desktop before visible native benchmarks'}
    $report = Join-Path $outputPath "$($case.Name).json"
    $arguments = @('--fixture', "$($case.Count)", '--flow', "$($case.Flow)", '--bench-output', $report, '--seconds', "$Seconds")
    if ($case.Scroll) { $arguments += '--scroll' }
    if ($case.Reduced) { $arguments += '--reduced-motion' }
    if ($case.Width) { $arguments += @('--width', "$($case.Width)", '--height', "$($case.Height)") }
    $started = [DateTime]::UtcNow
    $quotedArguments=$arguments | ForEach-Object { '"' + $_ + '"' }
    $nativeProcess = Start-Process -FilePath $binaryPath -ArgumentList $quotedArguments -PassThru -Wait -RedirectStandardError (Join-Path $outputPath "$($case.Name).stderr.log")
    if ($nativeProcess.ExitCode -ne 0 -or !(Test-Path -LiteralPath $report)) { throw "Native benchmark failed: $($case.Name)" }
    $result = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if ($result.session_initially_locked -ne $false -or $result.session_locked_at_end -or ($result.events | Where-Object { $_ -match 'session-locked' })) {throw "Desktop locked during benchmark: $($case.Name)"}
    [PSCustomObject]@{Case=$case.Name; StartUTC=$started.ToString('o'); SceneP95ms=$result.scene_cpu.p95_ms; IntervalP95ms=$result.frame_callback_interval.p95_ms; PeakMiB=$result.peak_working_set_mib; Cells=$result.visible_cells_peak; Cache=$result.cached_items_peak; Frames=$result.frames; GridBuilds=$result.grid_builds; Exit=$nativeProcess.ExitCode} | ConvertTo-Json -Compress
}
