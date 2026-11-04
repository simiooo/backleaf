param([Parameter(Mandatory=$true)][string]$Commit)
$ErrorActionPreference = 'Stop'
if ($Commit -notmatch '^[0-9a-f]{40}$') {throw 'Expected a full Git source commit'}
$projectRoot=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$cargoManifest=Get-Content -LiteralPath (Join-Path $projectRoot 'Cargo.toml') -Raw
$match=[regex]::Match($cargoManifest,'(?m)^version\s*=\s*"([^"]+)"')
if (!$match.Success) {throw 'Missing package version'}
$version=$match.Groups[1].Value
$build=Get-Content -LiteralPath (Join-Path $projectRoot '.local/native-build-manifest.json') -Raw | ConvertFrom-Json
if ($build.source_commit -ne $Commit -or $build.version -ne $version) {throw 'Build manifest does not match requested source/version'}
$builtHash=(Get-FileHash -LiteralPath (Join-Path $projectRoot 'target/release/backleaf.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
if ($builtHash -ne $build.exe_sha256) {throw 'Executable no longer matches recorded build'}
$short=$Commit.Substring(0,8)
$bundle=Join-Path $projectRoot ".local/native-bundles/$version-$short"
$artifact=Join-Path $projectRoot ".local/artifacts/Backleaf-$version-candidate-$short-win-x64.zip"
$staging=Join-Path $projectRoot ".local/staging/$version-$short"
if (Test-Path -LiteralPath $bundle) {throw 'Candidate bundle already exists; use a new source commit'}
New-Item -ItemType Directory -Path $bundle -Force | Out-Null
New-Item -ItemType Directory -Path (Split-Path -Parent $artifact) -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/backleaf.exe') -Destination (Join-Path $bundle 'Backleaf.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/native-preview-readme.txt') -Destination (Join-Path $bundle 'README.txt')
Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination (Join-Path $bundle 'LICENSE')
Copy-Item -LiteralPath (Join-Path $projectRoot '.local/native-build-manifest.json') -Destination (Join-Path $bundle 'build.json')
Copy-Item -LiteralPath (Join-Path $projectRoot '.local/native-licenses') -Destination (Join-Path $bundle 'licenses') -Recurse
$exeHash=(Get-FileHash -LiteralPath (Join-Path $bundle 'Backleaf.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
$metadata=[ordered]@{version=$version;source_commit=$Commit;gpui_revision='d36a70661471ec64dce230798121d554930d5c60';exe_sha256=$exeHash;packaged_at_utc=[DateTime]::UtcNow.ToString('o');status='candidate; final visible performance and interactive staging smoke pending'}
$metadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $bundle 'version.json') -Encoding utf8
Compress-Archive -Path (Join-Path $bundle '*') -DestinationPath $artifact -CompressionLevel Optimal
Expand-Archive -LiteralPath $artifact -DestinationPath $staging
$installedHash=(Get-FileHash -LiteralPath (Join-Path $staging 'Backleaf.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
if ($installedHash -ne $exeHash) {throw 'Installed executable does not match the built candidate'}
$sessionFile=Join-Path $staging 'session-status.json'
$nativeProcess=Start-Process -FilePath (Join-Path $staging 'Backleaf.exe') -ArgumentList @('--session-status-file',('"'+$sessionFile+'"')) -Wait -PassThru -WindowStyle Hidden
if ($nativeProcess.ExitCode -ne 0 -or !(Test-Path -LiteralPath $sessionFile)) {throw 'Installed candidate CLI smoke failed'}
$deployment=[ordered]@{version=$version;source_commit=$Commit;deployed_at_utc=[DateTime]::UtcNow.ToString('o');installation='portable ZIP extracted to isolated staging directory';staging_path=$staging;artifact_path=$artifact;artifact_sha256=(Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant();exe_sha256=$exeHash;installed_hash_matches=$true;cli_smoke_passed=$true;interactive_smoke_passed=$false;session=(Get-Content -LiteralPath $sessionFile -Raw | ConvertFrom-Json)}
$deployment | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $projectRoot '.local/native-staging-candidate.json') -Encoding utf8
$deployment | ConvertTo-Json -Depth 4
