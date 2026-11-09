[CmdletBinding()]
param([string]$Output = '.local/uninstall-tests.json')
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $project 'uninstall.ps1')
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ('backleaf-uninstall-test-' + [Guid]::NewGuid().ToString('N'))
$results = New-Object 'System.Collections.Generic.List[object]'
$registryKeys = New-Object 'System.Collections.Generic.List[string]'

function Assert-True($Value, [string]$Message) { if (!$Value) { throw $Message } }
function Expect-Failure([scriptblock]$Action, [string]$Pattern) {
    $failed = $false
    try { & $Action | Out-Null } catch {
        $failed = $true
        if ($_.Exception.Message -notmatch $Pattern) { throw "Unexpected failure: $($_.Exception.Message)" }
    }
    Assert-True $failed 'Expected an error, but the uninstaller succeeded.'
}
function Test-Case([string]$Name, [scriptblock]$Action) {
    & $Action
    $results.Add([PSCustomObject]@{ name = $Name; passed = $true })
    Write-Host "PASS $Name"
}
function New-Fixture {
    $id = [Guid]::NewGuid().ToString('N')
    $base = Join-Path $sandbox $id
    $root = Join-Path $base 'local\Programs\Backleaf'
    $current = Join-Path $root 'current'
    New-Item -ItemType Directory -Path $current -Force | Out-Null
    $exe = Join-Path $current 'Backleaf.exe'
    [IO.File]::WriteAllText($exe, 'fake installed executable')
    @{ application = 'Backleaf'; version = '0.1.0-preview.54' } | ConvertTo-Json |
        Set-Content -LiteralPath (Join-Path $current '.backleaf-install.json') -Encoding UTF8
    $programs = Join-Path $base 'programs'
    $desktop = Join-Path $base 'desktop'
    $pinned = Join-Path $base 'pinned'
    $data = Join-Path $base 'local\Backleaf'
    foreach ($directory in @($programs, $desktop, $pinned, (Join-Path $data 'operations'))) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $other = Join-Path $base 'other.exe'
    [IO.File]::WriteAllText($other, 'not backleaf')
    $shell = New-Object -ComObject WScript.Shell
    foreach ($directory in @($programs, $desktop)) {
        $shortcut = $shell.CreateShortcut((Join-Path $directory 'Backleaf.lnk'))
        $shortcut.TargetPath = $exe
        $shortcut.Save()
        $otherShortcut = $shell.CreateShortcut((Join-Path $directory 'Other.lnk'))
        $otherShortcut.TargetPath = $other
        $otherShortcut.Save()
    }
    $pinnedShortcut = $shell.CreateShortcut((Join-Path $pinned 'Backleaf.lnk'))
    $pinnedShortcut.TargetPath = $exe
    $pinnedShortcut.Save()
    [IO.File]::WriteAllText((Join-Path $root 'keep.txt'), 'kept because it is not installer-owned')
    [IO.File]::WriteAllText((Join-Path $data 'operations\marker.txt'), 'user data')
    $contextRoot = 'HKCU:\Software\BackleafUninstallTest-' + $id
    $registryKeys.Add($contextRoot)
    New-Item -Path (Join-Path $contextRoot 'Directory\shell\Backleaf\command') -Force | Out-Null
    Set-ItemProperty -LiteralPath (Join-Path $contextRoot 'Directory\shell\Backleaf\command') -Name '(default)' -Value ('"' + $exe + '" --path "%1"')
    New-Item -Path (Join-Path $contextRoot 'Directory\shell\Other\command') -Force | Out-Null
    Set-ItemProperty -LiteralPath (Join-Path $contextRoot 'Directory\shell\Other\command') -Name '(default)' -Value 'other.exe --path "%1"'
    [PSCustomObject]@{
        Root = $root
        Exe = $exe
        Programs = $programs
        Desktop = $desktop
        Pinned = $pinned
        Data = $data
        ContextRoot = $contextRoot
        Other = $other
    }
}
function Invoke-Uninstall($fixture, [switch]$RemoveUserData) {
    $arguments = @{
        InstallDir = $fixture.Root
        ProgramsDirectory = $fixture.Programs
        DesktopDirectory = $fixture.Desktop
        PinnedDirectory = $fixture.Pinned
        UserDataDirectory = $fixture.Data
        ContextMenuRoot = $fixture.ContextRoot
    }
    if ($RemoveUserData) { $arguments.RemoveUserData = $true }
    Uninstall-Backleaf @arguments
}

try {
    New-Item -ItemType Directory -Path $sandbox -Force | Out-Null
    Test-Case 'unmanaged directories are refused and left untouched' {
        $fixture = New-Fixture
        Remove-Item -LiteralPath (Join-Path $fixture.Root 'current/.backleaf-install.json') -Force
        Expect-Failure { Invoke-Uninstall $fixture } 'not a Backleaf installation managed by the installer'
        Assert-True (Test-Path -LiteralPath $fixture.Exe) 'Unmanaged executable was removed.'
        Assert-True (Test-Path -LiteralPath (Join-Path $fixture.Programs 'Backleaf.lnk')) 'Unmanaged shortcut was removed.'
    }
    Test-Case 'owned shortcuts and menu entries are removed while user data, unrelated files and unowned shortcuts stay' {
        $fixture = New-Fixture
        Invoke-Uninstall $fixture
        Assert-True (!(Test-Path -LiteralPath (Join-Path $fixture.Root 'current'))) 'Installed current directory was kept.'
        Assert-True (!(Test-Path -LiteralPath (Join-Path $fixture.Programs 'Backleaf.lnk'))) 'Start Menu shortcut was kept.'
        Assert-True (!(Test-Path -LiteralPath (Join-Path $fixture.Desktop 'Backleaf.lnk'))) 'Desktop shortcut was kept.'
        Assert-True (!(Test-Path -LiteralPath (Join-Path $fixture.Pinned 'Backleaf.lnk'))) 'Pinned taskbar shortcut was kept.'
        Assert-True (Test-Path -LiteralPath (Join-Path $fixture.Programs 'Other.lnk')) 'Unowned Start Menu shortcut was removed.'
        Assert-True (Test-Path -LiteralPath (Join-Path $fixture.Desktop 'Other.lnk')) 'Unowned desktop shortcut was removed.'
        Assert-True (Test-Path -LiteralPath (Join-Path $fixture.Root 'keep.txt')) 'Unrelated install-root file was removed.'
        Assert-True (Test-Path -LiteralPath (Join-Path $fixture.Data 'operations\marker.txt')) 'User data was removed without -RemoveUserData.'
        Assert-True (!(Test-Path -LiteralPath (Join-Path $fixture.ContextRoot 'Directory\shell\Backleaf'))) 'Owned Explorer menu entry was kept.'
        Assert-True (Test-Path -LiteralPath (Join-Path $fixture.ContextRoot 'Directory\shell\Other')) 'Unrelated Explorer menu entry was removed.'
    }
    Test-Case 'RemoveUserData deletes the user data directory' {
        $fixture = New-Fixture
        Invoke-Uninstall $fixture -RemoveUserData
        Assert-True (!(Test-Path -LiteralPath (Join-Path $fixture.Data 'operations'))) 'User data was kept with -RemoveUserData.'
        Assert-True (!(Test-Path -LiteralPath (Join-Path $fixture.Root 'current'))) 'Installed current directory was kept.'
    }
    Test-Case 'a running executable blocks uninstall without deleting anything' {
        $fixture = New-Fixture
        $guard = $null
        try {
            $guard = [IO.File]::Open($fixture.Exe, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            Expect-Failure { Invoke-Uninstall $fixture } 'Close Backleaf before uninstalling'
            Assert-True (Test-Path -LiteralPath $fixture.Exe) 'Running executable was removed.'
            Assert-True (Test-Path -LiteralPath (Join-Path $fixture.Programs 'Backleaf.lnk')) 'Shortcut was removed before the running guard.'
        } finally {
            if ($guard) { $guard.Dispose() }
        }
    }
    Test-Case 'install and user-data directories may not overlap' {
        $fixture = New-Fixture
        Expect-Failure {
            Uninstall-Backleaf -InstallDir $fixture.Data -UserDataDirectory $fixture.Data -ProgramsDirectory $fixture.Programs -DesktopDirectory $fixture.Desktop -PinnedDirectory $fixture.Pinned -ContextMenuRoot $fixture.ContextRoot
        } 'separate from Backleaf user data'
        Assert-True (Test-Path -LiteralPath (Join-Path $fixture.Data 'operations\marker.txt')) 'User data was touched by the refused uninstall.'
    }
    Test-Case 'a drive root install directory is refused' {
        Expect-Failure {
            Uninstall-Backleaf -InstallDir ([IO.Path]::GetPathRoot($env:SystemDrive)) -UserDataDirectory (Join-Path $sandbox 'nope') -ProgramsDirectory (Join-Path $sandbox 'nope-programs') -DesktopDirectory (Join-Path $sandbox 'nope-desktop') -PinnedDirectory (Join-Path $sandbox 'nope-pinned') -ContextMenuRoot (Join-Path $sandbox 'nope-context')
        } 'dedicated application directory'
    }
} finally {
    foreach ($key in $registryKeys) {
        if (Test-Path -LiteralPath $key) { Remove-Item -LiteralPath $key -Recurse -Force -ErrorAction SilentlyContinue }
    }
    if (Test-Path -LiteralPath $sandbox) { Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue }
}
$report = [ordered]@{
    passed = @($results | Where-Object { !$_.passed }).Count -eq 0
    test_count = $results.Count
    cases = $results
    reported_at_utc = [DateTime]::UtcNow.ToString('o')
}
$parent = Split-Path -Parent ([IO.Path]::GetFullPath($Output))
if ($parent) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
$report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Output -Encoding utf8
Get-Content -LiteralPath $Output -Raw
if (!$report.passed) { throw 'Uninstaller regressions failed.' }
