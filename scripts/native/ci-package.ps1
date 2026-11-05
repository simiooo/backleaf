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
Copy-Item docs/native-preview-readme.txt "$bundle/README.txt"
Copy-Item LICENSE "$bundle/LICENSE"
Copy-Item assets/brand/backleaf-logo.png "$bundle/Logo.png"
Copy-Item design/Backleaf-brand-v2.fig "$bundle/Backleaf-brand-v2.fig"
Copy-Item assets/brand/generation.md "$bundle/Logo-provenance.md"
Copy-Item design/Backleaf-v43.fig "$bundle/Backleaf-v43.fig"
Copy-Item design/Backleaf-v43.fig .local/ci/Backleaf-v43.fig
Copy-Item .local/ci/headless.json "$bundle/headless.json"
python scripts/native/collect-licenses.py "$bundle/licenses"
if ($LASTEXITCODE) { throw 'License collection failed' }
. ./install.ps1
New-Item -ItemType Directory -Force .local/ci/runtime-download | Out-Null
$runtime = Add-BackleafRuntime -Work ([IO.Path]::GetFullPath('.local/ci/runtime-download')) -Stage ([IO.Path]::GetFullPath($bundle))
$runtimeVersion=[Version](($runtime.files | Where-Object name -eq 'vcruntime140.dll').version)
if(!$env:VCToolsVersion -or $runtimeVersion -lt [Version]($env:VCToolsVersion.TrimEnd('\','/'))) {throw 'App-local runtime is older than the MSVC build toolset'}
$exeHash = (Get-FileHash "$bundle/Backleaf.exe" -Algorithm SHA256).Hash.ToLowerInvariant()
$metadata = [ordered]@{version=$version;source_commit=$commit;gpui_revision=(git -C vendor/zed rev-parse HEAD).Trim();built_at_utc=[DateTime]::UtcNow.ToString('o');exe_sha256=$exeHash;archive_library_sha256=(Get-FileHash "$bundle/lib/7z.dll").Hash.ToLowerInvariant();design_sha256=(Get-FileHash design/Backleaf-v43.fig).Hash.ToLowerInvariant();ci_run_url="https://github.com/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID";runtime=$runtime;scope='Native Windows preview with atomic directory/search refresh, stable menu windows and app-local runtime; see README for complete scope and limits';interactive_smoke='not run on hosted CI; local scope is recorded separately'}
$metadata.headless_test_count=$headless.test_count
$metadata.msvc_tools_version=$env:VCToolsVersion
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
& ./scripts/native/test-runtime.ps1 -InstallDirectory ([IO.Path]::GetFullPath($installed)) | Out-Null
$sessionFile = Join-Path (Get-Location) '.local/ci/session.json'
$p = Start-Process "$installed/Backleaf.exe" -ArgumentList @('--session-status-file',('"'+$sessionFile+'"')) -PassThru -Wait -WindowStyle Hidden
if ($p.ExitCode -ne 0 -or !(Test-Path $sessionFile)) { throw 'Packaged executable CLI smoke failed' }
$metadata.cli_smoke_passed = $true
$metadata | ConvertTo-Json -Depth 8 | Set-Content .local/ci/build.json -Encoding utf8
$sum = (Get-FileHash $zip).Hash.ToLowerInvariant() + '  ' + (Split-Path $zip -Leaf)
$sum | Set-Content .local/ci/SHA256SUMS.txt -Encoding ascii
((Get-FileHash .local/ci/Backleaf-v43.fig).Hash.ToLowerInvariant()+'  Backleaf-v43.fig') | Add-Content .local/ci/SHA256SUMS.txt -Encoding ascii
@{tag="test-v$version-$short";title="Backleaf $version · 原生测试包"} | ConvertTo-Json | Set-Content .local/ci/release.json -Encoding utf8
