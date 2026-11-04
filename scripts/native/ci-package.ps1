$ErrorActionPreference = 'Stop'
$version = ([regex]::Match((Get-Content Cargo.toml -Raw),'(?m)^version = "([^"]+)"')).Groups[1].Value
$commit = (git rev-parse HEAD).Trim()
$short = $commit.Substring(0,8)
$bundle = '.local/ci/Backleaf'
New-Item -ItemType Directory -Force $bundle | Out-Null
Copy-Item target/release/backleaf.exe "$bundle/Backleaf.exe"
Copy-Item docs/native-preview-readme.txt "$bundle/README.txt"
Copy-Item LICENSE "$bundle/LICENSE"
Copy-Item assets/brand/backleaf-logo.png "$bundle/Logo.png"
python scripts/native/collect-licenses.py "$bundle/licenses"
if ($LASTEXITCODE) { throw 'License collection failed' }
$exeHash = (Get-FileHash "$bundle/Backleaf.exe" -Algorithm SHA256).Hash.ToLowerInvariant()
$metadata = [ordered]@{version=$version;source_commit=$commit;gpui_revision=(git -C vendor/zed rev-parse HEAD).Trim();built_at_utc=[DateTime]::UtcNow.ToString('o');exe_sha256=$exeHash;ci_run_url="https://github.com/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID";scope='Logo integration on verified preview.2; file operations/search WIP excluded';interactive_smoke='not run on hosted CI'}
$metadata | ConvertTo-Json | Set-Content "$bundle/build.json" -Encoding utf8
Copy-Item "$bundle/build.json" .local/ci/build.json
$zip = ".local/ci/Backleaf-$version-$short-win-x64.zip"
Compress-Archive -Path "$bundle/*" -DestinationPath $zip
$installed = '.local/ci/installed'
Expand-Archive -LiteralPath $zip -DestinationPath $installed
if ((Get-FileHash "$installed/Backleaf.exe").Hash.ToLowerInvariant() -ne $exeHash) { throw 'ZIP executable hash mismatch' }
$sessionFile = Join-Path (Get-Location) '.local/ci/session.json'
$p = Start-Process "$installed/Backleaf.exe" -ArgumentList @('--session-status-file',('"'+$sessionFile+'"')) -PassThru -Wait -WindowStyle Hidden
if ($p.ExitCode -ne 0 -or !(Test-Path $sessionFile)) { throw 'Packaged executable CLI smoke failed' }
$metadata.cli_smoke_passed = $true
$metadata | ConvertTo-Json | Set-Content .local/ci/build.json -Encoding utf8
$sum = (Get-FileHash $zip).Hash.ToLowerInvariant() + '  ' + (Split-Path $zip -Leaf)
$sum | Set-Content .local/ci/SHA256SUMS.txt -Encoding ascii
@{tag="test-v$version-$short";title="Backleaf $version · Logo 测试包"} | ConvertTo-Json | Set-Content .local/ci/release.json -Encoding utf8
