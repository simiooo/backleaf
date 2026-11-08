[CmdletBinding()]
param([string]$PackageDirectory = '.local/ci', [string]$Work = '', [switch]$SkipLaunch)
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Push-Location $project
try {
    . ./install.ps1
    $metadata = Get-Content (Join-Path $PackageDirectory 'build-arm64.json') -Raw | ConvertFrom-Json
    $version = $metadata.version
    $short = $metadata.source_commit.Substring(0,8)
    $tag = "test-v$version-$short"
    $zipName = "Backleaf-$version-$short-win-arm64.zip"
    $names = @($zipName, 'build-arm64.json', 'SHA256SUMS-arm64.txt')
    foreach ($name in $names) {
        if (!(Test-Path -LiteralPath (Join-Path $PackageDirectory $name))) { throw "Missing ARM64 installer fixture: $name" }
    }
    $work = if ($Work) { [IO.Path]::GetFullPath($Work) } else { Join-Path $env:TEMP ('backleaf-arm64-installer-' + [Guid]::NewGuid().ToString('N')) }
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
    New-Item -ItemType Directory -Path $work | Out-Null
    $oldLocal = $env:LOCALAPPDATA
    $env:LOCALAPPDATA = Join-Path $work 'localappdata'
    New-Item -ItemType Directory -Path $env:LOCALAPPDATA | Out-Null
    $env:PROCESSOR_ARCHITECTURE = 'ARM64'
    $script:localAssets = [IO.Path]::GetFullPath($PackageDirectory)
    $script:localTag = $tag
    function Get-BackleafRelease {
        param([string]$Tag)
        $assets = foreach ($name in $names) {
            $file = Get-Item -LiteralPath (Join-Path $script:localAssets $name)
            [PSCustomObject]@{
                name = $name
                state = 'uploaded'
                size = $file.Length
                digest = 'sha256:' + (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                browser_download_url = "https://github.com/simiooo/backleaf/releases/download/$script:localTag/$name"
            }
        }
        [PSCustomObject]@{ tag_name = $script:localTag; draft = $false; published_at = '2026-11-08T00:00:00Z'; assets = @($assets) }
    }
    function Invoke-BackleafDownload {
        param($Uri, $Destination)
        $name = ([Uri]$Uri).Segments[-1]
        Copy-Item -LiteralPath (Join-Path $script:localAssets $name) -Destination $Destination
    }
    $target = Join-Path $work 'Programs/Backleaf'
    $receipt = Install-Backleaf -Tag $tag -InstallDir $target -NoShortcut
    if ($receipt.architecture -ne 'arm64' -or $receipt.version -ne $version) { throw 'ARM64 installer selected the wrong package.' }
    if ($SkipLaunch) {
        Write-Output "ARM64 installer selection passed: $tag ($($receipt.architecture))"
        return
    }
    $session = Join-Path $work 'session.json'
    $process = Start-Process -FilePath (Join-Path $target 'current/Backleaf.exe') -ArgumentList @('--session-status-file',('"'+$session+'"')) -PassThru -Wait -WindowStyle Hidden
    if ($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $session)) { throw 'Installed ARM64 executable CLI smoke failed.' }
    Write-Output "ARM64 installer smoke passed: $tag ($($receipt.architecture))"
} finally {
    if ($oldLocal) { $env:LOCALAPPDATA = $oldLocal }
    if ($work -and (Test-Path -LiteralPath $work)) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
    Pop-Location
}
