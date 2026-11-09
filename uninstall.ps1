# Backleaf per-user uninstaller. Compatible with Windows PowerShell 5.1 and PowerShell 7.
[CmdletBinding()]
param(
    [string]$InstallDir,
    [switch]$RemoveUserData,
    [string]$ProgramsDirectory,
    [string]$DesktopDirectory,
    [string]$PinnedDirectory,
    [string]$UserDataDirectory,
    [string]$ContextMenuRoot = 'HKCU:\Software\Classes'
)

function Uninstall-Backleaf {
    [CmdletBinding()]
    param(
        [string]$InstallDir,
        [switch]$RemoveUserData,
        [string]$ProgramsDirectory,
        [string]$DesktopDirectory,
        [string]$PinnedDirectory,
        [string]$UserDataDirectory,
        [string]$ContextMenuRoot = 'HKCU:\Software\Classes'
    )
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Backleaf uninstall requires Windows.' }
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw 'Windows LOCALAPPDATA is unavailable. Run the uninstaller in a signed-in Windows user session.' }
    if (!$InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\Backleaf' }
    $root = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
    if (!$root -or $root -eq [IO.Path]::GetPathRoot($root).TrimEnd('\')) { throw 'InstallDir must be a dedicated application directory.' }
    if (!$UserDataDirectory) { $UserDataDirectory = Join-Path $env:LOCALAPPDATA 'Backleaf' }
    $data = [IO.Path]::GetFullPath($UserDataDirectory).TrimEnd('\')
    if ($root -eq $data -or
        $root.StartsWith($data + '\', [StringComparison]::OrdinalIgnoreCase) -or
        $data.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'InstallDir must be separate from Backleaf user data.'
    }
    $current = Join-Path $root 'current'
    $receiptPath = Join-Path $current '.backleaf-install.json'
    if (!(Test-Path -LiteralPath $receiptPath)) { throw 'This directory is not a Backleaf installation managed by the installer.' }
    try { $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json } catch { throw 'The Backleaf install receipt is unreadable.' }
    if ($receipt.application -ne 'Backleaf') { throw 'This directory is not a Backleaf installation managed by the installer.' }
    $exe = [IO.Path]::GetFullPath((Join-Path $current 'Backleaf.exe'))

    function Get-FullPathOrNull([string]$Path) {
        try { [IO.Path]::GetFullPath($Path) } catch { $null }
    }
    function Get-ShortcutTarget([string]$Path) {
        $shell = New-Object -ComObject WScript.Shell
        try {
            $shortcut = $shell.CreateShortcut($Path)
            if ([string]::IsNullOrWhiteSpace($shortcut.TargetPath)) { return $null }
            return Get-FullPathOrNull $shortcut.TargetPath
        } catch { return $null }
    }
    function Remove-OwnedTree([string]$Path) {
        $full = [IO.Path]::GetFullPath($Path)
        if (!(Test-Path -LiteralPath $full)) { return }
        $rootItem = Get-Item -LiteralPath $full -Force
        if ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing to delete a linked path: $full" }
        foreach ($item in Get-ChildItem -LiteralPath $full -Force -Recurse) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing to delete a tree containing a link: $($item.FullName)" }
        }
        Remove-Item -LiteralPath $full -Recurse -Force
    }
    function Remove-OwnedShortcut([string]$Path, [string]$Target) {
        if (!(Test-Path -LiteralPath $Path)) { return $false }
        $actual = Get-ShortcutTarget $Path
        if (!$actual -or $actual -ne $Target) { return $false }
        Remove-Item -LiteralPath $Path -Force
        return $true
    }

    if (!$ProgramsDirectory) { $ProgramsDirectory = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs' }
    if (!$DesktopDirectory) { $DesktopDirectory = [Environment]::GetFolderPath('DesktopDirectory') }
    if (!$PinnedDirectory) { $PinnedDirectory = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar' }

    # An executing image cannot be opened for writing; refuse instead of leaving a broken install.
    $guard = $null
    if (Test-Path -LiteralPath $exe) {
        try { $guard = [IO.File]::Open($exe, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Delete) }
        catch [IO.IOException] {
            if (($_.Exception.HResult -band 0xffff) -in @(32, 33)) { throw 'Close Backleaf before uninstalling, then run the uninstaller again. The installed executable is in use.' }
            throw "Cannot lock the installed Backleaf executable for removal: $($_.Exception.Message)"
        }
        catch { throw "Cannot lock the installed Backleaf executable for removal: $($_.Exception.Message)" }
    }

    $removed = New-Object 'System.Collections.Generic.List[string]'
    try {
        foreach ($directory in @($ProgramsDirectory, $DesktopDirectory)) {
            $shortcut = Join-Path $directory 'Backleaf.lnk'
            if (Remove-OwnedShortcut $shortcut $exe) { $removed.Add($shortcut) }
        }
        # Pinned taskbar entries are shell-managed: ask Explorer to unpin first so the
        # taskband cache is updated, then remove a leftover link if needed.
        if (Test-Path -LiteralPath $PinnedDirectory) {
            $shell = New-Object -ComObject Shell.Application
            $folder = $shell.Namespace($PinnedDirectory)
            foreach ($file in @(Get-ChildItem -LiteralPath $PinnedDirectory -Filter '*.lnk' -Force -ErrorAction SilentlyContinue)) {
                if ((Get-ShortcutTarget $file.FullName) -ne $exe) { continue }
                if ($folder) {
                    $item = $folder.ParseName($file.Name)
                    if ($item) {
                        $verb = @($item.Verbs()) | Where-Object { $_.Name -match '取消固定|Unpin' } | Select-Object -First 1
                        if ($verb) {
                            $verb.DoIt()
                            for ($attempt = 0; $attempt -lt 20 -and (Test-Path -LiteralPath $file.FullName); $attempt++) { Start-Sleep -Milliseconds 100 }
                        }
                    }
                }
                if (Test-Path -LiteralPath $file.FullName) { Remove-Item -LiteralPath $file.FullName -Force }
                $removed.Add($file.FullName)
            }
        }
        foreach ($relative in @('Directory\shell\Backleaf', 'Drive\shell\Backleaf', 'Directory\Background\shell\Backleaf')) {
            $key = Join-Path $ContextMenuRoot $relative
            if (!(Test-Path -LiteralPath $key)) { continue }
            $commandPath = Join-Path $key 'command'
            $command = $null
            try { $command = (Get-ItemProperty -LiteralPath $commandPath -Name '(default)' -ErrorAction Stop).'(default)' } catch { $command = $null }
            if ($command -and ([string]$command).IndexOf($exe, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                Remove-Item -LiteralPath $key -Recurse -Force
                $removed.Add($key)
            }
        }
        foreach ($name in @('current', '.install.lock')) {
            Remove-OwnedTree (Join-Path $root $name)
        }
        foreach ($item in @(Get-ChildItem -LiteralPath $root -Force -ErrorAction SilentlyContinue)) {
            if ($item.Name -match '^\.(install|previous)-[a-f0-9]{32}$') { Remove-OwnedTree $item.FullName }
        }
        if ($guard) { $guard.Dispose(); $guard = $null }
        if ((@(Get-ChildItem -LiteralPath $root -Force -ErrorAction SilentlyContinue)).Count -eq 0) {
            Remove-Item -LiteralPath $root -Force
            Write-Host "Removed $root"
        } else {
            Write-Warning "Kept files not owned by the installer under $root"
        }
        if ($RemoveUserData) {
            if (Test-Path -LiteralPath $data) {
                Remove-OwnedTree $data
                Write-Host "Removed Backleaf user data: $data"
            }
        } else {
            Write-Host "Backleaf user data was preserved: $data"
            Write-Host 'Use -RemoveUserData to delete it as well.'
        }
        Write-Host "Backleaf uninstalled: $($removed.Count) shortcut/menu item(s) removed."
    } finally {
        if ($guard) { $guard.Dispose() }
    }
}

# Dot-sourcing exposes the uninstaller function for automated offline tests.
if ($MyInvocation.InvocationName -ne '.') { Uninstall-Backleaf @PSBoundParameters }
