$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$patch = Join-Path $root 'vendor/patches/backleaf-startup.patch'
$directWrite = Join-Path $root 'vendor/zed/crates/gpui_windows/src/direct_write.rs'
$platform = Join-Path $root 'vendor/zed/crates/gpui_windows/src/platform.rs'
if (!(Test-Path -LiteralPath $patch)) { throw "Missing vendored startup patch: $patch" }
if (!(Test-Path -LiteralPath $directWrite) -or !(Test-Path -LiteralPath $platform)) { throw 'Missing vendored GPUI Windows sources' }
$fontPatched = ([IO.File]::ReadAllText($directWrite)).Contains('Backleaf: no synchronous font-cache update check')
$gpuPatched = ([IO.File]::ReadAllText($platform)).Contains('backleaf-gpu-init')
if ($fontPatched -and $gpuPatched) {
    Write-Host 'Vendored startup patches already applied'
    exit 0
}
if ($fontPatched -or $gpuPatched) { throw 'Vendored startup patch is only partially applied; restore vendor/zed before packaging' }
git -C (Join-Path $root 'vendor/zed') apply --check $patch
if ($LASTEXITCODE) { throw 'Vendored startup patch does not apply cleanly' }
git -C (Join-Path $root 'vendor/zed') apply $patch
if ($LASTEXITCODE) { throw 'Failed to apply vendored startup patch' }
if (!(([IO.File]::ReadAllText($directWrite)).Contains('Backleaf: no synchronous font-cache update check')) -or
    !(([IO.File]::ReadAllText($platform)).Contains('backleaf-gpu-init'))) {
    throw 'Vendored startup patch verification failed'
}
Write-Host 'Applied vendored startup patches'
