param([Parameter(Mandatory)][string]$ExpectedSource,[string]$NotesFile='docs/native-test-release.md',[switch]$VerifyOnly)
$ErrorActionPreference='Stop'
$metadata=Get-Content .local/ci/build.json -Raw | ConvertFrom-Json
$release=Get-Content .local/ci/release.json -Raw | ConvertFrom-Json
if ($ExpectedSource -notmatch '^[0-9a-f]{40}$' -or $metadata.source_commit -ne $ExpectedSource -or !$metadata.cli_smoke_passed) { throw 'Build provenance or packaged smoke missing' }
$expectedTag="test-v$($metadata.version)-$($ExpectedSource.Substring(0,8))"
if ($release.tag -ne $expectedTag) { throw 'Release tag/source mismatch' }
$files=@()
foreach($line in Get-Content .local/ci/SHA256SUMS.txt) {
    if ($line -notmatch '^([a-f0-9]{64})  ([A-Za-z0-9._-]+)$') { throw 'Invalid checksum entry' }
    $file=Join-Path .local/ci $Matches[2]
    if ((Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant() -ne $Matches[1]) { throw 'Artifact checksum mismatch' }
    $files+=Get-Item -LiteralPath $file
}
if ($files.Count -ne 2 -or @($files|Where-Object Extension -eq '.zip').Count -ne 1 -or @($files|Where-Object Extension -eq '.fig').Count -ne 1) { throw 'Expected exactly ZIP and design checksums' }
$archive=[IO.Compression.ZipFile]::OpenRead(($files|Where-Object Extension -eq '.zip').FullName)
try {
    $required=@(@('Backleaf.exe',$metadata.exe_sha256),@(($files|Where-Object Extension -eq '.fig').Name,$metadata.design_sha256))
    if($metadata.archive_library_sha256){$required+=,@('lib/7z.dll',$metadata.archive_library_sha256)}
    if(!$metadata.headless_report_sha256 -or $metadata.headless_test_count -lt 16){throw 'Headless gate metadata missing'}
    $required+=,@('headless.json',$metadata.headless_report_sha256)
    foreach($pair in $required) {
        $entry=$archive.GetEntry($pair[0]);if(!$entry){throw 'Required packaged asset missing'}
        $stream=$entry.Open()
        try{$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}finally{$stream.Dispose()}
        if($hash -ne $pair[1]){throw 'Packaged asset hash mismatch'}
    }
} finally {$archive.Dispose()}
if($VerifyOnly){Write-Output 'CI artifact provenance and hashes verified';return}

function Retry-Gh([string[]]$Arguments) {
    for($attempt=0;$attempt -lt 4;$attempt++) {
        & gh @Arguments
        if($LASTEXITCODE -eq 0){return}
        if($attempt -lt 3){Start-Sleep -Seconds ([Math]::Pow(2,$attempt+1))}
    }
    throw "GitHub request failed after retries: $($Arguments[0..1] -join ' ')"
}
$existing=& gh release view $release.tag --json isDraft,targetCommitish,assets 2>$null
if($LASTEXITCODE -ne 0) {
    for($attempt=0;$attempt -lt 4;$attempt++) {
        & gh release create $release.tag --draft --prerelease --target $ExpectedSource --title $release.title --notes-file $NotesFile
        $existing=& gh release view $release.tag --json isDraft,targetCommitish,assets 2>$null
        if($LASTEXITCODE -eq 0){break}
        if($attempt -lt 3){Start-Sleep -Seconds ([Math]::Pow(2,$attempt+1))}
    }
    if($LASTEXITCODE){throw 'Cannot inspect created draft'}
}
$existing=$existing|ConvertFrom-Json
if($existing.targetCommitish -ne $ExpectedSource){throw 'Existing release points to a different source'}
$assets=@($files)+@(Get-Item .local/ci/SHA256SUMS.txt,.local/ci/build.json)
foreach($asset in $assets) {
    $digest='sha256:'+(Get-FileHash -LiteralPath $asset.FullName).Hash.ToLowerInvariant()
    $remote=@($existing.assets|Where-Object name -eq $asset.Name)
    if($remote.Count -eq 1 -and $remote[0].digest -eq $digest){continue}
    if(!$existing.isDraft){throw 'Published release has different assets; refusing to overwrite'}
    Retry-Gh -Arguments @('release','upload',$release.tag,$asset.FullName,'--clobber')
}
$verified=& gh release view $release.tag --json assets
if($LASTEXITCODE){throw 'Cannot verify uploaded assets'}
$verified=$verified|ConvertFrom-Json
foreach($asset in $assets) {
    $digest='sha256:'+(Get-FileHash -LiteralPath $asset.FullName).Hash.ToLowerInvariant()
    $remote=@($verified.assets|Where-Object name -eq $asset.Name)
    if($remote.Count -ne 1 -or $remote[0].digest -ne $digest){throw 'Remote asset digest mismatch'}
}
if($existing.isDraft){Retry-Gh -Arguments @('release','edit',$release.tag,'--draft=false')}
Write-Output "Published verified CI artifact: $($release.tag)"
