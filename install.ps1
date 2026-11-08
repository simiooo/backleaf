# Backleaf per-user installer. Compatible with Windows PowerShell 5.1 and PowerShell 7.
[CmdletBinding()]
param(
    [string]$Tag,
    [string]$InstallDir,
    [switch]$NoShortcut,
    [switch]$DesktopShortcut,
    [switch]$Launch
)

function Get-BackleafRelease {
    param([string]$Tag)
    $headers = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'Backleaf-Installer'; 'X-GitHub-Api-Version' = '2022-11-28' }
    $api = 'https://api.github.com/repos/simiooo/backleaf/releases'
    if ($Tag) {
        if ($Tag -notmatch '^test-v[0-9]+\.[0-9]+\.[0-9]+-preview\.[0-9]+-[a-f0-9]{8}$') { throw 'Invalid Backleaf release tag.' }
        $release = Invoke-RestMethod -Uri "$api/tags/$Tag" -Headers $headers -TimeoutSec 30 -ErrorAction Stop
    } else {
        # /latest excludes prereleases. Select the newest published Backleaf test package.
        $releases = Invoke-RestMethod -Uri "${api}?per_page=100" -Headers $headers -TimeoutSec 30 -ErrorAction Stop
        $release = @($releases | Where-Object { !$_.draft -and $_.tag_name -match '^test-v[0-9]+\.[0-9]+\.[0-9]+-preview\.[0-9]+-[a-f0-9]{8}$' } |
            Sort-Object { [DateTimeOffset]$_.published_at } -Descending | Select-Object -First 1)
        if ($release.Count -ne 1) { throw 'No published Backleaf test package found.' }
        $release = $release[0]
    }
    if ($release.draft) { throw 'Draft releases cannot be installed.' }
    return $release
}

function Save-BackleafAsset {
    param($Asset, [string]$Destination)
    if ($Asset.digest -notmatch '^sha256:[a-f0-9]{64}$' -or $Asset.size -le 0 -or $Asset.size -gt 256MB) { throw 'Release asset has no SHA256 digest or exceeds the size limit.' }
    $uri = [Uri]$Asset.browser_download_url
    if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'github.com' -or !$uri.AbsolutePath.StartsWith('/simiooo/backleaf/releases/download/')) { throw 'Unexpected release download URL.' }
    Write-Host "Downloading $($Asset.name)..."
    Invoke-BackleafDownload $uri.AbsoluteUri $Destination
    if ((Get-Item -LiteralPath $Destination).Length -ne $Asset.size -or
        ('sha256:' + (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()) -ne $Asset.digest) {
        throw "Release asset integrity check failed: $($Asset.name)"
    }
}

function Invoke-BackleafDownload {
    param([string]$Uri, [string]$Destination)
    # Windows' bundled curl has bounded redirects/timeouts across both PowerShell versions.
    $curl = Get-Command curl.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($curl) {
        & $curl.Source --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --retry 1 --connect-timeout 15 --max-time 900 --speed-limit 1024 --speed-time 30 --max-filesize 268435456 --output $Destination -- $Uri
        if ($LASTEXITCODE -ne 0) { throw "Release download failed (curl exit $LASTEXITCODE). Retry the install command." }
    } else {
        Invoke-WebRequest -Uri $Uri -UseBasicParsing -OutFile $Destination -TimeoutSec 60 -ErrorAction Stop
    }
}

function Add-BackleafRuntime {
    param([string]$Work, [string]$Stage)
    $runtimeTag = 'runtime-v2'
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/simiooo/backleaf/releases/tags/$runtimeTag" -Headers @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'Backleaf-Installer' } -TimeoutSec 30 -ErrorAction Stop
    if ($release.draft -or $release.tag_name -ne $runtimeTag) { throw 'Published Backleaf runtime support package is unavailable.' }
    $asset = @($release.assets | Where-Object { $_.name -ceq 'Backleaf-runtime-x64.zip' -and $_.state -eq 'uploaded' })
    if ($asset.Count -ne 1) { throw 'Runtime support asset is missing or ambiguous.' }
    $archive = Join-Path $Work 'Backleaf-runtime-x64.zip'
    Save-BackleafAsset $asset[0] $archive
    $runtime = Join-Path $Work 'runtime'
    Expand-BackleafPackage $archive $runtime
    $manifest = Get-Content -LiteralPath (Join-Path $runtime 'runtime.json') -Raw | ConvertFrom-Json
    if ($manifest.id -ne $runtimeTag -or $manifest.architecture -ne 'x64' -or !$manifest.files -or @($manifest.files).Count -gt 32) { throw 'Runtime support manifest mismatch.' }
    $names = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $manifest.files) {
        if ($file.name -notmatch '^(msvcp140(?:_[a-z0-9_]+)?|vcruntime140(?:_[a-z0-9_]+)?|concrt140|vccorlib140)\.dll$' -or !$names.Add($file.name) -or $file.sha256 -notmatch '^[a-f0-9]{64}$') { throw 'Invalid runtime DLL manifest entry.' }
        $source = Join-Path $runtime $file.name
        if ((Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant() -ne $file.sha256) { throw 'Runtime DLL integrity mismatch.' }
    }
    foreach ($required in @('msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll')) {
        if (!$names.Contains($required)) { throw "Required app-local runtime missing: $required" }
    }
    foreach ($file in $manifest.files) { Copy-Item -LiteralPath (Join-Path $runtime $file.name) -Destination (Join-Path $Stage $file.name) }
    $notice = Join-Path $Stage 'licenses/MSVC-runtime'
    New-Item -ItemType Directory -Path $notice -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $runtime 'runtime.json'),(Join-Path $runtime 'NOTICE.txt') -Destination $notice
    return [PSCustomObject]@{ tag = $runtimeTag; asset_sha256 = $asset[0].digest; files = @($manifest.files) }
}

function Assert-BackleafPath {
    param([string]$Root, [string]$Path)
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    $full = [IO.Path]::GetFullPath($Path)
    if (!$full.StartsWith($rootFull + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Installer path escapes its installation directory.' }
    # Never traverse junctions/symlinks when replacing or cleaning installer-owned trees.
    $walk = $full
    while ($walk) {
        if (Test-Path -LiteralPath $walk) {
            if ((Get-Item -LiteralPath $walk -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Installer paths must not contain junctions or symbolic links.' }
        }
        $parent = [IO.Path]::GetDirectoryName($walk)
        if ($parent -eq $walk) { break }
        $walk = $parent
    }
    return $full
}

function Remove-BackleafWork {
    param([string]$Root, [string]$Path)
    $full = Assert-BackleafPath $Root $Path
    if ((Split-Path $full -Leaf) -notmatch '^\.(install|previous)-[a-f0-9]{32}$') { throw 'Refusing to remove a non-installer temporary directory.' }
    if (Test-Path -LiteralPath $full) {
        foreach ($item in Get-ChildItem -LiteralPath $full -Force -Recurse) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refusing to clean a tree containing a link.' }
        }
        Remove-Item -LiteralPath $full -Recurse -Force
    }
}

function Expand-BackleafPackage {
    param([string]$Archive, [string]$Destination)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $names = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
        [long]$total = 0
        if ($zip.Entries.Count -gt 10000) { throw 'Package contains too many entries.' }
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\', '/')
            $parts = $name.TrimEnd('/').Split('/')
            if (!$name -or $name.StartsWith('/') -or $name.Contains(':') -or
                ($parts | Where-Object { !$_ -or $_ -eq '.' -or $_ -eq '..' -or $_ -match '[. ]$|[<>"|?*\x00-\x1f]|^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)' }) -or
                !$names.Add($name.TrimEnd('/')) -or (($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000 -or ($entry.ExternalAttributes -band 0x400)) {
                throw 'Unsafe or ambiguous path in package.'
            }
            $total += $entry.Length
            if ($total -gt 1GB) { throw 'Package exceeds the extraction size limit.' }
        }
    } finally { $zip.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Destination)
}

function Register-BackleafContextMenu {
    param([string]$Executable)
    # HKCU shell verbs need no elevation and are easy to overwrite on upgrade.
    $items = @(
        @{ Key = 'Software\Classes\Directory\shell\Backleaf'; Command = ('"' + $Executable + '" "%1"') },
        @{ Key = 'Software\Classes\Drive\shell\Backleaf'; Command = ('"' + $Executable + '" "%1"') },
        @{ Key = 'Software\Classes\Directory\Background\shell\Backleaf'; Command = ('"' + $Executable + '" "%V"') }
    )
    foreach ($item in $items) {
        $key = 'HKCU:\' + $item.Key
        New-Item -Path $key -Force | Out-Null
        Set-ItemProperty -LiteralPath $key -Name '(default)' -Value '在 Backleaf 中打开'
        Set-ItemProperty -LiteralPath $key -Name 'Icon' -Value $Executable
        New-Item -Path ($key + '\command') -Force | Out-Null
        Set-ItemProperty -LiteralPath ($key + '\command') -Name '(default)' -Value $item.Command
    }
}

function Install-Backleaf {
    [CmdletBinding()]
    param([string]$Tag, [string]$InstallDir,
        [switch]$NoShortcut, [switch]$DesktopShortcut, [switch]$Launch)
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    if ([Environment]::OSVersion.Platform -ne 'Win32NT' -or ![Environment]::Is64BitOperatingSystem) { throw 'Backleaf requires 64-bit Windows.' }
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw 'Windows LOCALAPPDATA is unavailable. Run the installer in a signed-in Windows user session.' }
    if (!$InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\Backleaf' }
    $root = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
    if (!$root -or $root -eq [IO.Path]::GetPathRoot($root).TrimEnd('\')) { throw 'InstallDir must be a dedicated application directory.' }
    $data = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Backleaf')).TrimEnd('\')
    if ($root -eq $data -or $root.StartsWith($data + '\', [StringComparison]::OrdinalIgnoreCase) -or $data.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'InstallDir must be separate from Backleaf user data.' }
    $current = Assert-BackleafPath $root (Join-Path $root 'current')
    New-Item -ItemType Directory -Path $root -Force | Out-Null
    # Cross-process exclusive lock; leave the harmless zero-byte lock file in place.
    $lock = $null
    try { $lock = [IO.File]::Open((Assert-BackleafPath $root (Join-Path $root '.install.lock')), 'OpenOrCreate', 'ReadWrite', 'None') }
    catch { throw 'Another installation is running, or the installation directory is not writable.' }
    $id = [Guid]::NewGuid().ToString('N')
    $work = Assert-BackleafPath $root (Join-Path $root ".install-$id")
    $backup = Assert-BackleafPath $root (Join-Path $root ".previous-$id")
    $committed = $false
    $executableGuard = $null
    $previousTls = [Net.ServicePointManager]::SecurityProtocol
    try {
        [Net.ServicePointManager]::SecurityProtocol = $previousTls -bor [Net.SecurityProtocolType]::Tls12
        if (Test-Path -LiteralPath $current) {
            $receiptPath = Join-Path $current '.backleaf-install.json'
            if (!(Test-Path -LiteralPath $receiptPath) -or (Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json).application -ne 'Backleaf') { throw 'Existing current directory is not managed by the Backleaf installer.' }
            # An executing image cannot be opened for writing. Lock the installed
            # image itself, not every same-named process (which can be a dead object
            # with an unreadable Path). No bytes are written and no process is killed.
            # Deny new readers/launches until promotion, but permit our directory
            # rename, rollback and backup deletion while this handle is held.
            $installedExe = Assert-BackleafPath $root (Join-Path $current 'Backleaf.exe')
            try { $executableGuard = [IO.File]::Open($installedExe, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Delete) }
            catch [IO.IOException] {
                if (($_.Exception.HResult -band 0xffff) -in @(32,33)) { throw 'Close Backleaf before upgrading, then run the same install command again. The installed executable is in use.' }
                throw "Cannot lock the installed Backleaf executable for upgrade: $($_.Exception.Message)"
            }
            catch { throw "Cannot lock the installed Backleaf executable for upgrade: $($_.Exception.Message)" }
        }
        Write-Host 'Finding Backleaf release...'
        $release = Get-BackleafRelease $Tag
        if ($release.tag_name -notmatch '^test-v([0-9]+\.[0-9]+\.[0-9]+-preview\.[0-9]+)-([a-f0-9]{8})$') { throw 'Unexpected release tag.' }
        $version = $Matches[1]; $sourceShort = $Matches[2]
        $zipName = "Backleaf-$version-$sourceShort-win-x64.zip"
        New-Item -ItemType Directory -Path $work | Out-Null
        foreach ($name in @($zipName, 'build.json', 'SHA256SUMS.txt')) {
            $asset = @($release.assets | Where-Object { $_.name -ceq $name -and $_.state -eq 'uploaded' })
            if ($asset.Count -ne 1) { throw "Release is missing a unique asset: $name" }
            Save-BackleafAsset $asset[0] (Join-Path $work $name)
        }
        $metadata = Get-Content -LiteralPath (Join-Path $work 'build.json') -Raw | ConvertFrom-Json
        if ($metadata.version -ne $version -or $metadata.source_commit -notmatch '^[a-f0-9]{40}$' -or
            !$metadata.source_commit.StartsWith($sourceShort) -or !$metadata.cli_smoke_passed) { throw 'Package source/version or CI smoke evidence mismatch.' }
        $checksum = @((Get-Content -LiteralPath (Join-Path $work 'SHA256SUMS.txt')) | Where-Object { $_ -cmatch ('^[a-f0-9]{64}  ' + [regex]::Escape($zipName) + '$') })
        if ($checksum.Count -ne 1 -or $checksum[0].Substring(0,64) -ne (Get-FileHash -LiteralPath (Join-Path $work $zipName)).Hash.ToLowerInvariant()) { throw 'Package checksum manifest mismatch.' }
        $stage = Join-Path $work 'package'
        Expand-BackleafPackage (Join-Path $work $zipName) $stage
        foreach ($pair in @(@('Backleaf.exe', $metadata.exe_sha256), @('lib\7z.dll', $metadata.archive_library_sha256))) {
            $path = Join-Path $stage $pair[0]
            if ($pair[1] -notmatch '^[a-f0-9]{64}$' -or !(Test-Path -LiteralPath $path) -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -ne $pair[1]) { throw 'Installed executable or archive library integrity mismatch.' }
        }
        $runtime = Add-BackleafRuntime $work $stage
        $receipt = [ordered]@{ application = 'Backleaf'; version = $version; tag = $release.tag_name; source_commit = $metadata.source_commit; installed_at_utc = [DateTime]::UtcNow.ToString('o'); exe_sha256 = $metadata.exe_sha256; runtime = $runtime }
        $receipt | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $stage '.backleaf-install.json') -Encoding UTF8
        # Same-volume renames. If promotion fails, restore the previous working install.
        if (Test-Path -LiteralPath $current) { Move-Item -LiteralPath $current -Destination $backup }
        try { Move-Item -LiteralPath $stage -Destination $current }
        catch {
            if (Test-Path -LiteralPath $backup) { Move-Item -LiteralPath $backup -Destination $current }
            throw
        }
        $committed = $true
        Remove-BackleafWork $root $backup
        if (!$NoShortcut) {
            try {
                $shell = New-Object -ComObject WScript.Shell
                $folders = @([Environment]::GetFolderPath('Programs'))
                if ($DesktopShortcut) { $folders += [Environment]::GetFolderPath('DesktopDirectory') }
                foreach ($folder in $folders) {
                    $shortcut = $shell.CreateShortcut((Join-Path $folder 'Backleaf.lnk'))
                    $shortcut.TargetPath = Join-Path $current 'Backleaf.exe'
                    $shortcut.WorkingDirectory = $current
                    $shortcut.Description = 'Backleaf'
                    $shortcut.Save()
                }
            } catch { Write-Warning "Installed successfully; shortcut could not be created: $($_.Exception.Message)" }
        }
        $defaultRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs\Backleaf')).TrimEnd('\')
        if ($root -eq $defaultRoot) {
            try { Register-BackleafContextMenu (Join-Path $current 'Backleaf.exe') }
            catch { Write-Warning "Installed successfully; Explorer context menu could not be registered: $($_.Exception.Message)" }
        }
        Write-Host "Installed Backleaf $version to $current"
        if ($Launch) { Start-Process -FilePath (Join-Path $current 'Backleaf.exe') }
        return [PSCustomObject]$receipt
    } catch {
        if ($committed) { throw "Backleaf was installed, but post-install cleanup failed: $($_.Exception.Message)" }
        throw
    } finally {
        try { Remove-BackleafWork $root $work } finally {
            if ($executableGuard) { $executableGuard.Dispose() }
            [Net.ServicePointManager]::SecurityProtocol = $previousTls
            $lock.Dispose()
        }
    }
}

# Dot-sourcing exposes the installer functions for automated offline tests.
if ($MyInvocation.InvocationName -ne '.') { Install-Backleaf @PSBoundParameters }
