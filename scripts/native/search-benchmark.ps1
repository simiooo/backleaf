param(
    [int]$Files=20000,
    [int]$Dirs=100,
    [int]$ColdRuns=3,
    [int]$WarmRuns=2,
    [string]$Output='.local/search-benchmark.json'
)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
Push-Location $projectRoot
try {
    $env:BACKLEAF_BENCH_FILES="$Files"
    $env:BACKLEAF_BENCH_DIRS="$Dirs"
    $env:BACKLEAF_BENCH_COLD_RUNS="$ColdRuns"
    $env:BACKLEAF_BENCH_WARM_RUNS="$WarmRuns"
    $env:BACKLEAF_BENCH_OUT=[IO.Path]::GetFullPath($Output)
    $logPath=[IO.Path]::ChangeExtension($env:BACKLEAF_BENCH_OUT,'log')
    $null=New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($logPath))
    cmd /d /c "cargo test --release --locked -p backleaf automatic_index_search_real_files_benchmark -- --ignored --nocapture > `"$logPath`" 2>&1"
    $code=$LASTEXITCODE
    if ($code -ne 0) { throw "Search benchmark failed with exit code $code; log: $logPath" }
    $line=Select-String -LiteralPath $logPath -Pattern '^AUTO_INDEX_SEARCH_BENCH ' | Select-Object -Last 1
    if (!$line) { throw "Search benchmark did not emit its report; log: $logPath" }
    Get-Content -LiteralPath $env:BACKLEAF_BENCH_OUT -Raw
    Write-Output "Search benchmark report: $env:BACKLEAF_BENCH_OUT"
    Write-Output "Search benchmark log: $logPath"
} finally { Pop-Location }
