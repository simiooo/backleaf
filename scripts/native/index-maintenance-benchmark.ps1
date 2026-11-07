param(
    [int]$Files=20000,
    [int]$Dirs=200,
    [int]$IdentitySample=200,
    [int]$IncrementalChanges=500,
    [string]$Output='.local/index-maintenance-benchmark.json'
)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
Push-Location $projectRoot
try {
    $env:BACKLEAF_MAINT_FILES="$Files"
    $env:BACKLEAF_MAINT_DIRS="$Dirs"
    $env:BACKLEAF_MAINT_IDENTITY_SAMPLE="$IdentitySample"
    $env:BACKLEAF_MAINT_INCR_CHANGES="$IncrementalChanges"
    $env:BACKLEAF_MAINT_OUT=[IO.Path]::GetFullPath($Output)
    $logPath=[IO.Path]::ChangeExtension($env:BACKLEAF_MAINT_OUT,'log')
    $null=New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($logPath))
    cmd /d /c "cargo test --release --locked -p backleaf index_maintenance_real_files_benchmark -- --ignored --nocapture > `"$logPath`" 2>&1"
    $code=$LASTEXITCODE
    if ($code -ne 0) { throw "Index maintenance benchmark failed with exit code $code; log: $logPath" }
    $line=Select-String -LiteralPath $logPath -Pattern '^AUTO_INDEX_MAINTENANCE_BENCH ' | Select-Object -Last 1
    if (!$line) { throw "Index maintenance benchmark did not emit its report; log: $logPath" }
    Get-Content -LiteralPath $env:BACKLEAF_MAINT_OUT -Raw
    Write-Output "Index maintenance report: $env:BACKLEAF_MAINT_OUT"
    Write-Output "Index maintenance log: $logPath"
} finally { Pop-Location }
