$ErrorActionPreference = 'Stop'
$version = ([regex]::Match((Get-Content Cargo.toml -Raw),'(?m)^version = "([^"]+)"')).Groups[1].Value
$commit = (git rev-parse HEAD).Trim()
$short = $commit.Substring(0,8)
$headless = Get-Content .local/ci/headless.json -Raw | ConvertFrom-Json
if (!$headless.passed -or $headless.workspace_commit -ne $commit -or $headless.workspace_dirty) {throw 'Clean-source headless interaction gate missing'}
$bundle = '.local/ci/Backleaf-arm64'
New-Item -ItemType Directory -Force $bundle | Out-Null
Copy-Item target/aarch64-pc-windows-msvc/release/backleaf.exe "$bundle/Backleaf.exe"
New-Item -ItemType Directory -Force "$bundle/lib" | Out-Null
Copy-Item assets/archive-engine/7z-arm64.dll "$bundle/lib/7z.dll"
$readme = if ($version -match '-preview\.') { 'docs/native-preview-readme.txt' } else { 'docs/release-readme.txt' }
Copy-Item $readme "$bundle/README.txt"
Copy-Item LICENSE "$bundle/LICENSE"
Copy-Item assets/brand/backleaf-logo.png "$bundle/Logo.png"
Copy-Item design/Backleaf-brand-v3.fig "$bundle/Backleaf-brand-v3.fig"
Copy-Item assets/brand/generation.md "$bundle/Logo-provenance.md"
Copy-Item design/Backleaf-v46.fig "$bundle/Backleaf-v46.fig"
Copy-Item design/Backleaf-v46.fig .local/ci/Backleaf-v46.fig
Copy-Item .local/ci/headless.json "$bundle/headless.json"
python scripts/native/collect-licenses.py "$bundle/licenses"
if ($LASTEXITCODE) { throw 'License collection failed' }

$redist = $env:VCToolsRedistDir
if (!$redist -or !(Test-Path -LiteralPath $redist)) { throw 'VCToolsRedistDir is unavailable on the ARM64 runner.' }
$crt = Get-ChildItem -LiteralPath $redist -Recurse -Directory -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\arm64\\' } | Sort-Object FullName | Select-Object -Last 1
if (!$crt) { throw 'ARM64 Microsoft VC runtime redistributable directory is unavailable.' }
function Test-Arm64Pe([string]$Path) {
    $bytes = [byte[]](Get-Content -LiteralPath $Path -AsByteStream -TotalCount 1024)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { return $false }
    $pe = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($pe -lt 0 -or $pe + 6 -ge $bytes.Length) { return $false }
    return ([BitConverter]::ToUInt16($bytes, $pe + 4) -eq 0xaa64)
}
$runtimeFiles = @()
foreach ($dll in Get-ChildItem -LiteralPath $crt.FullName -Filter '*.dll' | Sort-Object Name) {
    if (!(Test-Arm64Pe $dll.FullName)) {
        Write-Warning "Skipping non-ARM64 redistributable: $($dll.Name)"
        continue
    }
    Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $bundle $dll.Name)
    $runtimeFiles += [ordered]@{
        name = $dll.Name
        sha256 = (Get-FileHash -LiteralPath (Join-Path $bundle $dll.Name) -Algorithm SHA256).Hash.ToLowerInvariant()
        version = $dll.VersionInfo.FileVersion
    }
}
if ($runtimeFiles.Count -lt 2) { throw 'ARM64 runtime redistributable set is incomplete.' }
$runtime = [ordered]@{ tag = 'msvc-arm64-redist'; files = $runtimeFiles }

$exeHash = (Get-FileHash "$bundle/Backleaf.exe" -Algorithm SHA256).Hash.ToLowerInvariant()
$metadata = [ordered]@{
    version = $version
    source_commit = $commit
    architecture = 'arm64'
    application_icon = 'v3'
    logo_sha256 = (Get-FileHash "$bundle/Logo.png").Hash.ToLowerInvariant()
    brand_design_sha256 = (Get-FileHash "$bundle/Backleaf-brand-v3.fig").Hash.ToLowerInvariant()
    gpui_revision = (git -C vendor/zed rev-parse HEAD).Trim()
    built_at_utc = [DateTime]::UtcNow.ToString('o')
    exe_sha256 = $exeHash
    archive_library_sha256 = (Get-FileHash "$bundle/lib/7z.dll").Hash.ToLowerInvariant()
    design_sha256 = (Get-FileHash design/Backleaf-v46.fig).Hash.ToLowerInvariant()
    ci_run_url = "https://github.com/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"
    runtime = $runtime
    scope = 'Native Windows ARM64 preview with bounded persistent-index maintenance, default-search index acceleration, idle scheduling, on-demand roots and app-local ARM64 runtime; see README for complete scope and limits'
    interactive_smoke = 'not run on hosted CI; packaged ARM64 CLI smoke is recorded'
    headless_test_count = $headless.test_count
    msvc_tools_version = $env:VCToolsVersion
    headless_log_sha256 = $headless.log_sha256
    headless_report_sha256 = (Get-FileHash "$bundle/headless.json").Hash.ToLowerInvariant()
}
if ($env:BACKLEAF_VENDOR_PATCH) { $metadata.vendor_patch = $env:BACKLEAF_VENDOR_PATCH }
$metadata | ConvertTo-Json -Depth 8 | Set-Content "$bundle/build-arm64.json" -Encoding utf8
Copy-Item "$bundle/build-arm64.json" .local/ci/build-arm64.json
$zip = ".local/ci/Backleaf-$version-$short-win-arm64.zip"
Compress-Archive -Path "$bundle/*" -DestinationPath $zip
$installed = '.local/ci/installed-arm64'
if (Test-Path -LiteralPath $installed) { Remove-Item -LiteralPath $installed -Recurse -Force }
Expand-Archive -LiteralPath $zip -DestinationPath $installed
if ((Get-FileHash "$installed/Backleaf.exe").Hash.ToLowerInvariant() -ne $exeHash) { throw 'ARM64 ZIP executable hash mismatch' }
if ((Get-FileHash "$installed/lib/7z.dll").Hash.ToLowerInvariant() -ne $metadata.archive_library_sha256) { throw 'ARM64 ZIP archive library hash mismatch' }
if ((Get-FileHash "$installed/Logo.png").Hash.ToLowerInvariant() -ne $metadata.logo_sha256) { throw 'ARM64 ZIP logo hash mismatch' }
if ((Get-FileHash "$installed/Backleaf-brand-v3.fig").Hash.ToLowerInvariant() -ne $metadata.brand_design_sha256) { throw 'ARM64 ZIP brand design hash mismatch' }
& ./scripts/native/test-runtime.ps1 -InstallDirectory ([IO.Path]::GetFullPath($installed)) -Architecture arm64 | Out-Null
$sessionFile = Join-Path (Get-Location) '.local/ci/session-arm64.json'
$p = Start-Process "$installed/Backleaf.exe" -ArgumentList @('--session-status-file',('"'+$sessionFile+'"')) -PassThru -Wait -WindowStyle Hidden
if ($p.ExitCode -ne 0 -or !(Test-Path $sessionFile)) { throw 'Packaged ARM64 executable CLI smoke failed' }
$metadata.cli_smoke_passed = $true
$metadata | ConvertTo-Json -Depth 8 | Set-Content "$bundle/build-arm64.json" -Encoding utf8
$metadata | ConvertTo-Json -Depth 8 | Set-Content .local/ci/build-arm64.json -Encoding utf8
$sum = (Get-FileHash $zip).Hash.ToLowerInvariant() + '  ' + (Split-Path $zip -Leaf)
$sum | Set-Content .local/ci/SHA256SUMS-arm64.txt -Encoding ascii
((Get-FileHash .local/ci/Backleaf-v46.fig).Hash.ToLowerInvariant()+'  Backleaf-v46.fig') | Add-Content .local/ci/SHA256SUMS-arm64.txt -Encoding ascii
