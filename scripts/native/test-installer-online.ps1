[CmdletBinding()]
param([string]$Tag, [string]$Output = '.local/installer-online.json')
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $project 'install.ps1')
$root = Join-Path $project ('.local/installer-staging/' + $PSVersionTable.PSVersion.Major + '-' + [Guid]::NewGuid().ToString('N'))
$receipt = Install-Backleaf -Tag $Tag -InstallDir $root -NoShortcut
$runtimeLoads = & (Join-Path $PSScriptRoot 'test-runtime.ps1') -InstallDirectory (Join-Path $root 'current')
$exe = Join-Path $root 'current/Backleaf.exe'
$sessionFile = Join-Path $root 'session.json'
$process = Start-Process -FilePath $exe -ArgumentList @('--session-status-file', ('"' + $sessionFile + '"')) -WindowStyle Hidden -PassThru
if (!$process.WaitForExit(30000)) {
    # Only this installer-owned diagnostic process, never a user's running instance.
    $process.Kill()
    throw 'Installed diagnostic process timed out.'
}
if ($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $sessionFile)) { throw 'Installed executable diagnostic failed.' }
$hash = (Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant()
if ($hash -ne $receipt.exe_sha256) { throw 'Installed executable changed after installation.' }
$report = [ordered]@{ passed = $true; powershell = $PSVersionTable.PSVersion.ToString(); installer_sha256 = (Get-FileHash (Join-Path $project 'install.ps1')).Hash.ToLowerInvariant(); anonymous_download = $true
    tag = $receipt.tag; version = $receipt.version; source_commit = $receipt.source_commit; installed_path = $root
    installed_at_utc = $receipt.installed_at_utc; exe_sha256 = $hash; cli_smoke_passed = $true; runtime = $receipt.runtime; app_local_runtime_loads = @($runtimeLoads)
    gui_smoke = 'not run; installation-only change'; session = (Get-Content -LiteralPath $sessionFile -Raw | ConvertFrom-Json) }
$outputFull = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path (Split-Path $outputFull -Parent) -Force | Out-Null
$report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $outputFull -Encoding UTF8
Write-Host "Anonymous release installation and executable smoke passed: $($receipt.tag)"
