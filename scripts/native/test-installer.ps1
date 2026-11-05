[CmdletBinding()]
param([string]$Output = '.local/installer-tests.json')
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $project 'install.ps1')
Add-Type -AssemblyName System.IO.Compression.FileSystem
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ('backleaf-installer-test-' + [Guid]::NewGuid().ToString('N'))
$originalLocal = $env:LOCALAPPDATA
$results = New-Object 'System.Collections.Generic.List[object]'
$script:failPromotion = $false
$script:running = $false
$script:downloadFailure = $false
$originalPath = $env:PATH
$originalTls = [Net.ServicePointManager]::SecurityProtocol

function Assert-True($Value, [string]$Message) { if (!$Value) { throw $Message } }
function Expect-Failure([scriptblock]$Action, [string]$Pattern) {
    $failed = $false
    try { & $Action | Out-Null } catch {
        $failed = $true
        if ($_.Exception.Message -notmatch $Pattern) { throw "Unexpected failure: $($_.Exception.Message)" }
    }
    Assert-True $failed 'Expected an error, but installation succeeded.'
}
function Test-Case([string]$Name, [scriptblock]$Action) {
    & $Action
    $results.Add([PSCustomObject]@{ name = $Name; passed = $true })
    Write-Host "PASS $Name"
}
function New-Fixture([string]$Version = '0.1.0-preview.15', [string]$Content = 'first binary') {
    $dir = Join-Path $sandbox ('fixture-' + [Guid]::NewGuid().ToString('N'))
    $package = Join-Path $dir 'package'
    New-Item -ItemType Directory -Path (Join-Path $package 'lib') -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $package 'Backleaf.exe'), $Content)
    [IO.File]::WriteAllText((Join-Path $package 'lib/7z.dll'), 'archive library')
    $source = '786c6e53acd66e7e0a139b30aa6b4dc75e464897'
    $tag = "test-v$Version-786c6e53"
    $zipName = "Backleaf-$Version-786c6e53-win-x64.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory($package, (Join-Path $dir $zipName))
    @{ version = $Version; source_commit = $source; cli_smoke_passed = $true
        exe_sha256 = (Get-FileHash (Join-Path $package 'Backleaf.exe')).Hash.ToLowerInvariant()
        archive_library_sha256 = (Get-FileHash (Join-Path $package 'lib/7z.dll')).Hash.ToLowerInvariant()
    } | ConvertTo-Json | Set-Content (Join-Path $dir 'build.json') -Encoding UTF8
    ((Get-FileHash (Join-Path $dir $zipName)).Hash.ToLowerInvariant() + '  ' + $zipName) | Set-Content (Join-Path $dir 'SHA256SUMS.txt') -Encoding ASCII
    $assets = foreach ($name in @($zipName, 'build.json', 'SHA256SUMS.txt')) {
        $file = Get-Item (Join-Path $dir $name)
        [PSCustomObject]@{ name = $name; state = 'uploaded'; size = $file.Length
            digest = 'sha256:' + (Get-FileHash $file.FullName).Hash.ToLowerInvariant()
            browser_download_url = "https://github.com/simiooo/backleaf/releases/download/$tag/$name" }
    }
    return [PSCustomObject]@{ dir = $dir; release = [PSCustomObject]@{ tag_name = $tag; draft = $false; published_at = '2026-11-05T06:50:20Z'; assets = @($assets) } }
}

# Only network, process enumeration and one rename fault are substituted. All hashes,
# ZIP inspection/extraction, locks, install promotion, rollback and cleanup are real.
function Invoke-RestMethod {
    param($Uri, $Headers, $TimeoutSec, $ErrorAction)
    if ($Uri -like '*/tags/runtime-v1') { return $script:runtimeFixture.release }
    return $script:releases
}
function Invoke-BackleafDownload {
    param($Uri, $Destination)
    if ($script:downloadFailure) { throw 'Simulated download failure.' }
    $base = $script:fixture.dir
    if ($Uri -like '*/runtime-v1/*') { $base = $script:runtimeFixture.dir }
    Copy-Item -LiteralPath (Join-Path $base ([Uri]$Uri).Segments[-1]) -Destination $Destination
}
function Get-Process {
    param($Name, $ErrorAction)
    if ($script:running) { return [PSCustomObject]@{ Path = Join-Path $script:target 'current/Backleaf.exe' } }
}
function Move-Item {
    param($LiteralPath, $Destination)
    if ($script:failPromotion -and (Split-Path $LiteralPath -Leaf) -eq 'package') { throw 'Simulated promotion failure.' }
    Microsoft.PowerShell.Management\Move-Item -LiteralPath $LiteralPath -Destination $Destination
}

try {
    New-Item -ItemType Directory -Path $sandbox | Out-Null
    $env:LOCALAPPDATA = Join-Path $sandbox 'local app data'
    $runtimeDir = Join-Path $sandbox 'runtime-fixture'
    $runtimePackage = Join-Path $runtimeDir 'package'
    New-Item -ItemType Directory -Path $runtimePackage -Force | Out-Null
    $runtimeFiles = foreach ($name in @('msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll')) {
        $path = Join-Path $runtimePackage $name
        [IO.File]::WriteAllText($path, 'offline fixture runtime: ' + $name)
        @{name=$name;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
    }
    @{id='runtime-v1';architecture='x64';files=@($runtimeFiles)}|ConvertTo-Json -Depth 5|Set-Content (Join-Path $runtimePackage 'runtime.json') -Encoding UTF8
    'Offline runtime notice fixture' | Set-Content (Join-Path $runtimePackage 'NOTICE.txt')
    $runtimeZip = Join-Path $runtimeDir 'Backleaf-runtime-x64.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory($runtimePackage, $runtimeZip)
    $script:runtimeFixture = [PSCustomObject]@{dir=$runtimeDir;release=[PSCustomObject]@{tag_name='runtime-v1';draft=$false;assets=@([PSCustomObject]@{
        name='Backleaf-runtime-x64.zip';state='uploaded';size=(Get-Item -LiteralPath $runtimeZip).Length
        digest='sha256:'+(Get-FileHash -LiteralPath $runtimeZip).Hash.ToLowerInvariant()
        browser_download_url='https://github.com/simiooo/backleaf/releases/download/runtime-v1/Backleaf-runtime-x64.zip'
    })}}
    $script:target = Join-Path $env:LOCALAPPDATA 'Programs/Backleaf'
    $script:fixture = New-Fixture
    $script:releases = @($script:fixture.release)
    Test-Case 'latest prerelease is selected and drafts are ignored' {
        $draft = [PSCustomObject]@{ tag_name = 'test-v0.1.0-preview.99-aaaaaaaa'; draft = $true; published_at = '2099-01-01T00:00:00Z' }
        $script:releases = @($draft, $script:fixture.release)
        Assert-True ((Get-BackleafRelease).tag_name -eq $script:fixture.release.tag_name) 'Wrong release selected.'
        $script:releases = @($script:fixture.release)
    }
    Test-Case 'fresh install in a path containing spaces and Unicode' {
        $script:target = Join-Path $env:LOCALAPPDATA ('Programs/' + [char]0x62fe + [char]0x5f84 + ' Backleaf')
        $receipt = Install-Backleaf -InstallDir $script:target -NoShortcut
        Assert-True ($receipt.version -eq '0.1.0-preview.15') 'Wrong receipt.'
        Assert-True ((Get-Content (Join-Path $script:target 'current/Backleaf.exe') -Raw) -eq 'first binary') 'Incorrect installed bytes.'
    }
    Test-Case 'upgrade and repeat install preserve user data and unrelated root files' {
        $data = Join-Path $env:LOCALAPPDATA 'Backleaf/operations'
        New-Item -ItemType Directory -Path $data -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $data 'state.json'), 'user data sentinel')
        [IO.File]::WriteAllText((Join-Path $script:target 'personal.txt'), 'keep me')
        $script:fixture = New-Fixture '0.1.0-preview.16' 'second binary'
        $script:releases = @($script:fixture.release)
        1..2 | ForEach-Object { Install-Backleaf -InstallDir $script:target -NoShortcut | Out-Null }
        Assert-True ((Get-Content (Join-Path $script:target 'current/Backleaf.exe') -Raw) -eq 'second binary') 'Upgrade failed.'
        Assert-True ((Get-Content (Join-Path $data 'state.json') -Raw) -eq 'user data sentinel') 'User data changed.'
        Assert-True ((Get-Content (Join-Path $script:target 'personal.txt') -Raw) -eq 'keep me') 'Unrelated file changed.'
    }
    Test-Case 'running application blocks upgrade without termination' {
        $script:running = $true
        try { Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'Close Backleaf' } finally { $script:running = $false }
    }
    Test-Case 'failed download leaves the previous installation intact' {
        $script:downloadFailure = $true
        try { Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'download failure' } finally { $script:downloadFailure = $false }
        Assert-True ((Get-Content (Join-Path $script:target 'current/Backleaf.exe') -Raw) -eq 'second binary') 'Previous installation lost.'
    }
    Test-Case 'asset digest mismatch is rejected before replacing current' {
        $asset = $script:fixture.release.assets[0]; $old = $asset.digest
        $asset.digest = 'sha256:' + ('0' * 64)
        try { Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'integrity check failed' } finally { $asset.digest = $old }
    }
    Test-Case 'failed directory promotion restores the previous installation' {
        $script:failPromotion = $true
        try { Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'promotion failure' } finally { $script:failPromotion = $false }
        Assert-True ((Get-Content (Join-Path $script:target 'current/Backleaf.exe') -Raw) -eq 'second binary') 'Rollback did not restore installed bytes.'
    }
    Test-Case 'missing release assets are rejected' {
        $old = $script:fixture.release.assets
        $script:fixture.release.assets = @($old | Where-Object name -ne 'build.json')
        try { Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'missing a unique asset' } finally { $script:fixture.release.assets = $old }
    }
    Test-Case 'metadata source/version mismatch is rejected even with valid asset digests' {
        $path = Join-Path $script:fixture.dir 'build.json'
        $bytes = [IO.File]::ReadAllBytes($path)
        $asset = $script:fixture.release.assets | Where-Object name -eq 'build.json'
        $oldSize = $asset.size; $oldDigest = $asset.digest
        try {
            $metadata = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
            $metadata.source_commit = 'a' * 40
            $metadata | ConvertTo-Json | Set-Content -LiteralPath $path -Encoding UTF8
            $asset.size = (Get-Item -LiteralPath $path).Length
            $asset.digest = 'sha256:' + (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()
            Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'source/version'
        } finally { [IO.File]::WriteAllBytes($path, $bytes); $asset.size = $oldSize; $asset.digest = $oldDigest }
    }
    Test-Case 'inner executable mismatch is rejected even with valid ZIP and manifest digests' {
        $originalFixture = $script:fixture
        try {
            $script:fixture = New-Fixture '0.1.0-preview.17' 'third binary'
            $script:releases = @($script:fixture.release)
            $path = Join-Path $script:fixture.dir 'build.json'
            $metadata = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
            $metadata.exe_sha256 = '0' * 64
            $metadata | ConvertTo-Json | Set-Content -LiteralPath $path -Encoding UTF8
            $asset = $script:fixture.release.assets | Where-Object name -eq 'build.json'
            $asset.size = (Get-Item -LiteralPath $path).Length
            $asset.digest = 'sha256:' + (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()
            Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'executable or archive library'
        } finally { $script:fixture = $originalFixture; $script:releases = @($script:fixture.release) }
    }
    Test-Case 'unmanaged application directory is never overwritten' {
        $other = Join-Path $sandbox 'unmanaged'
        New-Item -ItemType Directory -Path (Join-Path $other 'current') -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $other 'current/keep.txt'), 'keep')
        Expect-Failure { Install-Backleaf -InstallDir $other -NoShortcut } 'not managed'
        Assert-True (Test-Path (Join-Path $other 'current/keep.txt')) 'Unmanaged file lost.'
    }
    Test-Case 'concurrent installation is refused' {
        $lock = [IO.File]::Open((Join-Path $script:target '.install.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
        try { Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'Another installation' } finally { $lock.Dispose() }
    }
    Test-Case 'root escape, user data target and unsafe cleanup are refused' {
        Expect-Failure { Assert-BackleafPath $script:target (Join-Path $script:target '../outside') } 'escapes'
        Expect-Failure { Install-Backleaf -InstallDir (Join-Path $env:LOCALAPPDATA 'Backleaf') -NoShortcut } 'separate'
        Expect-Failure { Remove-BackleafWork $script:target (Join-Path $script:target 'current') } 'non-installer'
    }
    Test-Case 'ZIP traversal, absolute paths, ADS and duplicate names are rejected' {
        foreach ($names in @(@('../escape.txt'), @('/absolute.txt'), @('file:stream'), @('one.txt', 'ONE.txt'), @('CON.txt'), @('folder./file'))) {
            $bad = Join-Path $sandbox ([Guid]::NewGuid().ToString('N') + '.zip')
            $zip = [IO.Compression.ZipFile]::Open($bad, 'Create')
            try { foreach ($name in $names) { $null = $zip.CreateEntry($name) } } finally { $zip.Dispose() }
            Expect-Failure { Expand-BackleafPackage $bad (Join-Path $sandbox 'unsafe-extract') } 'Unsafe'
        }
        Assert-True (!(Test-Path (Join-Path $sandbox 'unsafe-extract'))) 'Unsafe package was extracted.'
    }
    Test-Case 'junction installation paths are rejected' {
        $linked = Join-Path $sandbox 'linked'
        New-Item -ItemType Junction -Path $linked -Target $script:target | Out-Null
        try { Expect-Failure { Install-Backleaf -InstallDir $linked -NoShortcut } 'junctions' }
        finally { [IO.Directory]::Delete($linked) }
    }
    Test-Case 'temporary installation trees are removed after success and failure' {
        Assert-True (@(Get-ChildItem -LiteralPath $script:target -Force | Where-Object { $_.Name -match '^\.(install|previous)-' }).Count -eq 0) 'Temporary tree leaked.'
    }
    Test-Case 'runtime is installed app-local with license and receipt' {
        foreach ($name in @('msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll','licenses/MSVC-runtime/runtime.json','licenses/MSVC-runtime/NOTICE.txt')) {
            Assert-True (Test-Path -LiteralPath (Join-Path $script:target ('current/'+$name))) 'App-local runtime or license missing.'
        }
        $receipt = Get-Content -LiteralPath (Join-Path $script:target 'current/.backleaf-install.json') -Raw|ConvertFrom-Json
        Assert-True ($receipt.runtime.tag -eq 'runtime-v1') 'Runtime provenance missing.'
        Assert-True ($receipt.runtime.files[0].sha256 -match '^[a-f0-9]{64}$') 'Runtime receipt was truncated.'
    }
    Test-Case 'runtime download corruption prevents application replacement' {
        $asset = $script:runtimeFixture.release.assets[0]; $old = $asset.digest
        $asset.digest = 'sha256:' + ('0' * 64)
        try { Expect-Failure { Install-Backleaf -InstallDir $script:target -NoShortcut } 'integrity check failed' } finally { $asset.digest = $old }
        Assert-True ((Get-Content (Join-Path $script:target 'current/Backleaf.exe') -Raw) -eq 'second binary') 'Runtime failure replaced existing app.'
    }
    Test-Case 'missing LOCALAPPDATA is reported without changing environment' {
        $saved = $env:LOCALAPPDATA
        try { $env:LOCALAPPDATA = $null; Expect-Failure { Install-Backleaf -NoShortcut } 'LOCALAPPDATA is unavailable' }
        finally { $env:LOCALAPPDATA = $saved }
    }
    Test-Case 'installer preserves PATH and restores TLS configuration after success and errors' {
        Assert-True ($env:PATH -ceq $originalPath) 'PATH changed.'
        Assert-True ([Net.ServicePointManager]::SecurityProtocol -eq $originalTls) 'TLS configuration leaked.'
    }
    $report = [ordered]@{ passed = $true; powershell = $PSVersionTable.PSVersion.ToString(); installer_sha256 = (Get-FileHash (Join-Path $project 'install.ps1')).Hash.ToLowerInvariant(); test_count = $results.Count; tests = @($results.ToArray()); completed_at_utc = [DateTime]::UtcNow.ToString('o') }
    $outputFull = [IO.Path]::GetFullPath($Output)
    New-Item -ItemType Directory -Path (Split-Path $outputFull -Parent) -Force | Out-Null
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $outputFull -Encoding UTF8
} finally {
    $env:LOCALAPPDATA = $originalLocal
    # This is the exact directory created above, never a caller-supplied path.
    $resolved = [IO.Path]::GetFullPath($sandbox)
    if ($resolved.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path $resolved -Leaf) -match '^backleaf-installer-test-[a-f0-9]{32}$') {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
