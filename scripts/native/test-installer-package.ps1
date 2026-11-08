[CmdletBinding()]
param([string]$PackageDirectory='.local/ci', [ValidateSet('x64','arm64')][string]$Architecture='x64', [string]$Output='')
$ErrorActionPreference='Stop'
$project=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Push-Location $project
$oldLocal=$env:LOCALAPPDATA
$oldArchitecture=$env:PROCESSOR_ARCHITECTURE
try {
    . ./install.ps1
    $metadataName=if($Architecture -eq 'arm64'){'build-arm64.json'}else{'build.json'}
    $metadata=Get-Content -LiteralPath (Join-Path $PackageDirectory $metadataName) -Raw | ConvertFrom-Json
    $version=$metadata.version
    $short=$metadata.source_commit.Substring(0,8)
    $tag=if($version -match '-preview\.') {"test-v$version-$short"} else {"v$version"}
    $zipName="Backleaf-$version-$short-win-$Architecture.zip"
    $names=@($zipName,$metadataName,$(if($Architecture -eq 'arm64'){'SHA256SUMS-arm64.txt'}else{'SHA256SUMS.txt'}))
    $work=Join-Path $project ('.local/installer-package-'+[Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $work | Out-Null
    $script:packageAssets=[IO.Path]::GetFullPath($PackageDirectory)
    $script:packageTag=$tag
    $script:packageSource=$metadata.source_commit
    $script:packageVersion=$version
    $script:packageNames=$names
    $env:LOCALAPPDATA=Join-Path $work 'localappdata'
    $env:PROCESSOR_ARCHITECTURE=if($Architecture -eq 'arm64'){'ARM64'}else{'AMD64'}
    New-Item -ItemType Directory -Path $env:LOCALAPPDATA | Out-Null
    if($Architecture -eq 'x64') {
        # Exercise the real runtime installation with the exact packaged CRT DLLs.
        $runtimePackage=Join-Path $work 'runtime-support/package'
        New-Item -ItemType Directory -Path $runtimePackage -Force | Out-Null
        $installed=Join-Path $PackageDirectory 'installed'
        foreach($file in $metadata.runtime.files){Copy-Item -LiteralPath (Join-Path $installed $file.name) -Destination $runtimePackage}
        @{id='runtime-v2';architecture='x64';files=@($metadata.runtime.files)} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $runtimePackage 'runtime.json') -Encoding utf8
        'Offline installer smoke fixture: unmodified packaged Microsoft VC runtime.' | Set-Content -LiteralPath (Join-Path $runtimePackage 'NOTICE.txt')
        $script:runtimeArchive=Join-Path $work 'Backleaf-runtime-x64.zip'
        Compress-Archive -Path (Join-Path $runtimePackage '*') -DestinationPath $script:runtimeArchive
        $runtimeFile=Get-Item -LiteralPath $script:runtimeArchive
        $script:runtimeRelease=[PSCustomObject]@{tag_name='runtime-v2';draft=$false;assets=@([PSCustomObject]@{
            name='Backleaf-runtime-x64.zip';state='uploaded';size=$runtimeFile.Length
            digest='sha256:'+(Get-FileHash -LiteralPath $runtimeFile.FullName).Hash.ToLowerInvariant()
            browser_download_url='https://github.com/simiooo/backleaf/releases/download/runtime-v2/Backleaf-runtime-x64.zip'
        })}
    }
    function Get-BackleafRelease {
        param([string]$Tag)
        $assets=foreach($name in $script:packageNames){
            $file=Get-Item -LiteralPath (Join-Path $script:packageAssets $name)
            [PSCustomObject]@{name=$name;state='uploaded';size=$file.Length;digest='sha256:'+(Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant();browser_download_url="https://github.com/simiooo/backleaf/releases/download/$script:packageTag/$name"}
        }
        [PSCustomObject]@{tag_name=$script:packageTag;target_commitish=$script:packageSource;draft=$false;prerelease=($script:packageVersion -match '-preview\.');published_at='2026-11-08T00:00:00Z';assets=@($assets)}
    }
    function Invoke-RestMethod {
        param($Uri,$Headers,$TimeoutSec,$ErrorAction)
        if($Uri -like '*/tags/runtime-v2' -and $Architecture -eq 'x64'){return $script:runtimeRelease}
        throw 'Offline installer smoke attempted an unexpected network request.'
    }
    function Invoke-BackleafDownload {
        param($Uri,$Destination)
        if($Uri -like '*/runtime-v2/*'){Copy-Item -LiteralPath $script:runtimeArchive -Destination $Destination}
        else {Copy-Item -LiteralPath (Join-Path $script:packageAssets ([Uri]$Uri).Segments[-1]) -Destination $Destination}
    }
    $target=Join-Path $work 'Programs/Backleaf'
    $receipt=Install-Backleaf -Tag $tag -InstallDir $target -NoShortcut
    if($receipt.architecture -ne $Architecture -or $receipt.version -ne $version -or $receipt.source_commit -ne $metadata.source_commit){throw 'Installed package provenance mismatch'}
    $session=Join-Path $work 'session.json'
    $process=Start-Process -FilePath (Join-Path $target 'current/Backleaf.exe') -ArgumentList @('--session-status-file',('"'+$session+'"')) -Wait -PassThru -WindowStyle Hidden
    if($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $session)){throw 'Offline installed executable smoke failed'}
    if(!$Output){$Output=Join-Path $PackageDirectory "installer-package-$Architecture.json"}
    @{passed=$true;architecture=$Architecture;version=$version;source_commit=$metadata.source_commit;tag=$tag;cli_smoke_passed=$true;installed_at_utc=$receipt.installed_at_utc;exe_sha256=$receipt.exe_sha256;scope='Real Install-Backleaf with local CI assets, hash/ZIP/promotion/runtime paths and actual installed EXE CLI. Network is substituted; user default installation is untouched.'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Output -Encoding utf8
    Write-Output "Offline installed package smoke passed: $tag ($Architecture)"
} finally {
    $env:LOCALAPPDATA=$oldLocal
    $env:PROCESSOR_ARCHITECTURE=$oldArchitecture
    Pop-Location
}
