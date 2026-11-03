$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$dependencyPath = Join-Path $projectRoot 'vendor/zed'
$gpuiRevision = 'd36a70661471ec64dce230798121d554930d5c60'
if (!(Test-Path -LiteralPath (Join-Path $dependencyPath 'Cargo.toml'))) {
    git -C $projectRoot clone --depth 1 --filter=blob:none --no-checkout https://github.com/zed-industries/zed $dependencyPath
    if ($LASTEXITCODE) { throw 'GPUI clone failed' }
    git -C $dependencyPath sparse-checkout set crates
    git -C $dependencyPath fetch --depth 1 origin $gpuiRevision
    if ($LASTEXITCODE) { throw 'GPUI fetch failed' }
    git -C $dependencyPath checkout --detach $gpuiRevision
    if ($LASTEXITCODE) { throw 'GPUI checkout failed' }
    git -C $projectRoot submodule absorbgitdirs vendor/zed
}
$actualRevision = git -C $dependencyPath rev-parse HEAD
if ($actualRevision -ne $gpuiRevision) { throw "Unexpected GPUI revision: $actualRevision" }
Write-Output "GPUI pinned at $actualRevision. Run cargo build --release --locked."
