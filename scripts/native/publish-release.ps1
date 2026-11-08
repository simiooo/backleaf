param([Parameter(Mandatory)][string]$ExpectedSource, [string]$X64Directory='.local/release-x64', [string]$Arm64Directory='.local/release-arm64', [string]$NotesFile='docs/release-v0.1.0.md', [switch]$VerifyOnly)
$ErrorActionPreference='Stop'
if($ExpectedSource -cnotmatch '^[0-9a-f]{40}$'){throw 'Expected an immutable full source commit'}
$assets=@()
$version=$null
foreach($pair in @(@('x64',$X64Directory),@('arm64',$Arm64Directory))){
    $architecture=$pair[0];$directory=$pair[1]
    $metaName=if($architecture -eq 'arm64'){'build-arm64.json'}else{'build.json'}
    $sumsName=if($architecture -eq 'arm64'){'SHA256SUMS-arm64.txt'}else{'SHA256SUMS.txt'}
    $metadata=Get-Content -LiteralPath (Join-Path $directory $metaName) -Raw | ConvertFrom-Json
    if($metadata.version -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -or $metadata.source_commit -cne $ExpectedSource -or !$metadata.cli_smoke_passed -or $metadata.headless_test_count -lt 52){throw 'Stable build provenance or gates missing'}
    if($version -and $version -ne $metadata.version){throw 'Architecture versions differ'}
    $version=$metadata.version
    if($architecture -eq 'arm64' -and $metadata.architecture -ne 'arm64'){throw 'ARM64 architecture metadata mismatch'}
    $installer=Get-Content -LiteralPath (Join-Path $directory "installer-package-$architecture.json") -Raw | ConvertFrom-Json
    if(!$installer.passed -or !$installer.cli_smoke_passed -or $installer.architecture -ne $architecture -or $installer.source_commit -cne $ExpectedSource -or $installer.version -ne $version){throw 'Real installed-package smoke missing'}
    $zipName="Backleaf-$version-$($ExpectedSource.Substring(0,8))-win-$architecture.zip"
    $zipPath=Join-Path $directory $zipName
    $sumLines=@(Get-Content -LiteralPath (Join-Path $directory $sumsName))
    if($sumLines.Count -ne 2){throw 'Expected exactly package and UI-design checksums'}
    foreach($line in $sumLines){
        if($line -cnotmatch '^([a-f0-9]{64})  ([A-Za-z0-9._-]+)$'){throw 'Invalid checksum entry'}
        $file=Join-Path $directory $Matches[2]
        if((Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant() -ne $Matches[1]){throw 'Artifact checksum mismatch'}
    }
    if(!($sumLines -contains ((Get-FileHash -LiteralPath $zipPath).Hash.ToLowerInvariant()+'  '+$zipName))){throw 'Package missing from checksum manifest'}
    $archive=[IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($zipPath))
    try {
        $required=@(@('Backleaf.exe',$metadata.exe_sha256),@('lib/7z.dll',$metadata.archive_library_sha256),@('Backleaf-v46.fig',$metadata.design_sha256),@('Logo.png',$metadata.logo_sha256),@('Backleaf-brand-v3.fig',$metadata.brand_design_sha256),@('headless.json',$metadata.headless_report_sha256))
        if(@($metadata.runtime.files).Count -lt 3){throw 'App-local runtime metadata missing'}
        foreach($file in $metadata.runtime.files){$required+=,@($file.name,$file.sha256)}
        foreach($item in $required){
            if($item[1] -cnotmatch '^[a-f0-9]{64}$'){throw 'Missing packaged-file digest'}
            $entry=$archive.GetEntry($item[0]);if(!$entry){throw "Required file missing: $($item[0])"}
            $stream=$entry.Open()
            try{$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}finally{$stream.Dispose()}
            if($hash -ne $item[1]){throw "Packaged file mismatch: $($item[0])"}
        }
        $stream=$archive.GetEntry('headless.json').Open()
        $reader=[IO.StreamReader]::new($stream)
        try{$headless=$reader.ReadToEnd()|ConvertFrom-Json}finally{$reader.Dispose()}
        if(!$headless.passed -or $headless.workspace_dirty -or $headless.workspace_commit -cne $ExpectedSource -or $headless.test_count -lt 52){throw 'Clean-source headless report missing'}
    } finally {$archive.Dispose()}
    $assets+=@(Get-Item -LiteralPath $zipPath,(Join-Path $directory $metaName),(Join-Path $directory $sumsName))
}
$assets+=Get-Item -LiteralPath (Join-Path $X64Directory 'Backleaf-v46.fig')
if($VerifyOnly){Write-Output "Stable release ${version}: both architectures, packaged hashes and installer gates verified";return}
$tag="v$version"
function Invoke-ReleaseGh([string[]]$Arguments){
    for($attempt=0;$attempt -lt 4;$attempt++){
        & gh @Arguments
        if(!$LASTEXITCODE){return}
        if($attempt -lt 3){Start-Sleep -Seconds ([Math]::Pow(2,$attempt+1))}
    }
    throw "Release request failed: $($Arguments[0..1] -join ' ')"
}
$existing=& gh release view $tag --json isDraft,isPrerelease,targetCommitish,assets 2>$null
if($LASTEXITCODE){
    Invoke-ReleaseGh -Arguments @('release','create',$tag,'--draft','--target',$ExpectedSource,'--title',"Backleaf $version",'--notes-file',$NotesFile)
    $existing=& gh release view $tag --json isDraft,isPrerelease,targetCommitish,assets
    if($LASTEXITCODE){throw 'Cannot inspect stable release draft'}
}
$existing=$existing|ConvertFrom-Json
if($existing.targetCommitish -cne $ExpectedSource -or $existing.isPrerelease){throw 'Stable tag/source mismatch'}
foreach($asset in $assets){
    $digest='sha256:'+(Get-FileHash -LiteralPath $asset.FullName).Hash.ToLowerInvariant()
    $remote=@($existing.assets|Where-Object name -ceq $asset.Name)
    if($remote.Count -eq 1 -and $remote[0].digest -eq $digest){continue}
    if(!$existing.isDraft){throw 'Refusing to overwrite a published stable asset'}
    Invoke-ReleaseGh -Arguments @('release','upload',$tag,$asset.FullName,'--clobber')
}
$verified=& gh release view $tag --json assets
if($LASTEXITCODE){throw 'Cannot inspect uploaded stable assets'}
$verified=$verified|ConvertFrom-Json
foreach($asset in $assets){
    $digest='sha256:'+(Get-FileHash -LiteralPath $asset.FullName).Hash.ToLowerInvariant()
    $remote=@($verified.assets|Where-Object name -ceq $asset.Name)
    if($remote.Count -ne 1 -or $remote[0].digest -ne $digest){throw 'Uploaded stable asset digest mismatch'}
}
Invoke-ReleaseGh -Arguments @('release','edit',$tag,'--draft=false','--prerelease=false','--latest')
Write-Output "Published verified stable release: $tag"
