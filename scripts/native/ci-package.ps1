$ErrorActionPreference = 'Stop'
$version = ([regex]::Match((Get-Content Cargo.toml -Raw),'(?m)^version = "([^"]+)"')).Groups[1].Value
$commit = (git rev-parse HEAD).Trim()
$short = $commit.Substring(0,8)
$headless = Get-Content .local/ci/headless.json -Raw | ConvertFrom-Json
if (!$headless.passed -or $headless.workspace_commit -ne $commit -or $headless.workspace_dirty) {throw 'Clean-source headless interaction gate missing'}
$bundle = '.local/ci/Backleaf'
New-Item -ItemType Directory -Force $bundle | Out-Null
Copy-Item target/release/backleaf.exe "$bundle/Backleaf.exe"
New-Item -ItemType Directory -Force "$bundle/lib" | Out-Null
Copy-Item assets/archive-engine/7z.dll "$bundle/lib/7z.dll"
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
. ./install.ps1
$redist = $env:VCToolsRedistDir
if (!$redist -or !(Test-Path -LiteralPath $redist)) { throw 'VCToolsRedistDir is unavailable.' }
$crt = Get-ChildItem -LiteralPath $redist -Recurse -Directory -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName | Select-Object -Last 1
if (!$crt) { throw 'Microsoft VC x64 runtime redistributable directory is unavailable.' }
$runtimeFiles = @()
foreach ($dll in Get-ChildItem -LiteralPath $crt.FullName -Filter '*.dll' | Sort-Object Name) {
    Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $bundle $dll.Name)
    $runtimeFiles += [ordered]@{
        name = $dll.Name
        sha256 = (Get-FileHash -LiteralPath (Join-Path $bundle $dll.Name) -Algorithm SHA256).Hash.ToLowerInvariant()
        version = $dll.VersionInfo.FileVersion
    }
}
if ($runtimeFiles.Count -lt 3) { throw 'Microsoft VC x64 runtime redistributable set is incomplete.' }
$runtime = [ordered]@{ tag = 'msvc-x64-redist'; files = $runtimeFiles }
$exeHash = (Get-FileHash "$bundle/Backleaf.exe" -Algorithm SHA256).Hash.ToLowerInvariant()
$metadata = [ordered]@{version=$version;source_commit=$commit;gpui_revision=(git -C vendor/zed rev-parse HEAD).Trim();built_at_utc=[DateTime]::UtcNow.ToString('o');exe_sha256=$exeHash;archive_library_sha256=(Get-FileHash "$bundle/lib/7z.dll").Hash.ToLowerInvariant();design_sha256=(Get-FileHash design/Backleaf-v46.fig).Hash.ToLowerInvariant();ci_run_url="https://github.com/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID";runtime=$runtime;scope='Native Windows preview with bounded persistent-index maintenance, default-search index acceleration, idle scheduling, on-demand roots and app-local runtime; see README for complete scope and limits';interactive_smoke='not run on hosted CI; local scope is recorded separately'}
$metadata.headless_test_count=$headless.test_count
$metadata.application_icon='v3'
$metadata.logo_sha256=(Get-FileHash "$bundle/Logo.png").Hash.ToLowerInvariant()
$metadata.brand_design_sha256=(Get-FileHash "$bundle/Backleaf-brand-v3.fig").Hash.ToLowerInvariant()
$metadata.msvc_tools_version=$env:VCToolsVersion
if ($env:BACKLEAF_VENDOR_PATCH) { $metadata.vendor_patch = $env:BACKLEAF_VENDOR_PATCH }
$metadata.headless_log_sha256=$headless.log_sha256
$metadata.headless_report_sha256=(Get-FileHash "$bundle/headless.json").Hash.ToLowerInvariant()
$metadata | ConvertTo-Json -Depth 8 | Set-Content "$bundle/build.json" -Encoding utf8
Copy-Item "$bundle/build.json" .local/ci/build.json
$zip = ".local/ci/Backleaf-$version-$short-win-x64.zip"
Compress-Archive -Path "$bundle/*" -DestinationPath $zip
$installed = '.local/ci/installed'
Expand-Archive -LiteralPath $zip -DestinationPath $installed
if ((Get-FileHash "$installed/Backleaf.exe").Hash.ToLowerInvariant() -ne $exeHash) { throw 'ZIP executable hash mismatch' }
if ((Get-FileHash "$installed/lib/7z.dll").Hash.ToLowerInvariant() -ne $metadata.archive_library_sha256) { throw 'ZIP archive library hash mismatch' }
if ((Get-FileHash "$installed/Logo.png").Hash.ToLowerInvariant() -ne $metadata.logo_sha256) { throw 'ZIP logo hash mismatch' }
if ((Get-FileHash "$installed/Backleaf-brand-v3.fig").Hash.ToLowerInvariant() -ne $metadata.brand_design_sha256) { throw 'ZIP brand design hash mismatch' }
& ./scripts/native/test-runtime.ps1 -InstallDirectory ([IO.Path]::GetFullPath($installed)) | Out-Null
$sessionFile = Join-Path (Get-Location) '.local/ci/session.json'
$p = Start-Process "$installed/Backleaf.exe" -ArgumentList @('--session-status-file',('"'+$sessionFile+'"')) -PassThru -Wait -WindowStyle Hidden
if ($p.ExitCode -ne 0 -or !(Test-Path $sessionFile)) { throw 'Packaged executable CLI smoke failed' }
$metadata.cli_smoke_passed = $true
$metadata | ConvertTo-Json -Depth 8 | Set-Content .local/ci/build.json -Encoding utf8
$sum = (Get-FileHash $zip).Hash.ToLowerInvariant() + '  ' + (Split-Path $zip -Leaf)
$sum | Set-Content .local/ci/SHA256SUMS.txt -Encoding ascii
((Get-FileHash .local/ci/Backleaf-v46.fig).Hash.ToLowerInvariant()+'  Backleaf-v46.fig') | Add-Content .local/ci/SHA256SUMS.txt -Encoding ascii
$release = if ($version -match '-preview\.') { @{tag="test-v$version-$short";title="Backleaf $version · 原生测试包"} } else { @{tag="v$version";title="Backleaf $version"} }
$release | ConvertTo-Json | Set-Content .local/ci/release.json -Encoding utf8
