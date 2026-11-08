param([Parameter(Mandatory)][string]$ExpectedSource)
$ErrorActionPreference = 'Stop'
$metadata = Get-Content .local/ci/build-arm64.json -Raw | ConvertFrom-Json
if ($ExpectedSource -notmatch '^[0-9a-f]{40}$' -or $metadata.source_commit -ne $ExpectedSource -or !$metadata.cli_smoke_passed -or $metadata.architecture -ne 'arm64') { throw 'ARM64 build provenance or packaged smoke missing' }
$tag = "test-v$($metadata.version)-$($ExpectedSource.Substring(0,8))"
$existing = & gh release view $tag --json isDraft,targetCommitish,assets 2>$null
if ($LASTEXITCODE -ne 0) { throw 'The x64 job has not published the target release yet.' }
$existing = $existing | ConvertFrom-Json
if ($existing.targetCommitish -ne $ExpectedSource -or $existing.isDraft) { throw 'Existing release is not the published target for this source.' }
$zip = Get-Item ".local/ci/Backleaf-$($metadata.version)-$($ExpectedSource.Substring(0,8))-win-arm64.zip"
$sums = Get-Item .local/ci/SHA256SUMS-arm64.txt
$meta = Get-Item .local/ci/build-arm64.json
$expected = (Get-FileHash -LiteralPath $zip.FullName).Hash.ToLowerInvariant() + '  ' + $zip.Name
if (-not ((Get-Content -LiteralPath $sums.FullName) -contains $expected)) { throw 'ARM64 checksum manifest does not match the ZIP.' }
$assets = @($zip, $meta, $sums)
foreach ($asset in $assets) {
    $digest = 'sha256:' + (Get-FileHash -LiteralPath $asset.FullName).Hash.ToLowerInvariant()
    $remote = @($existing.assets | Where-Object name -eq $asset.Name)
    if ($remote.Count -eq 1 -and $remote[0].digest -eq $digest) { continue }
    & gh release upload $tag $asset.FullName --clobber
    if ($LASTEXITCODE) { throw "ARM64 asset upload failed: $($asset.Name)" }
}
$verified = (& gh release view $tag --json assets) | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'Cannot verify ARM64 release assets.' }
foreach ($asset in $assets) {
    $digest = 'sha256:' + (Get-FileHash -LiteralPath $asset.FullName).Hash.ToLowerInvariant()
    $remote = @($verified.assets | Where-Object name -eq $asset.Name)
    if ($remote.Count -ne 1 -or $remote[0].digest -ne $digest) { throw "ARM64 remote asset digest mismatch: $($asset.Name)" }
}
Write-Output "Published verified ARM64 assets for $tag"
