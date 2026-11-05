[CmdletBinding()]
param([string]$Output = '.local/runtime-ci', [string]$RuntimeTag = 'runtime-v2')
$ErrorActionPreference = 'Stop'
if ($RuntimeTag -notmatch '^runtime-v[0-9]+$') { throw 'Invalid runtime tag' }
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -prerelease -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vs) { throw 'Licensed Visual Studio C++ redistributable installation not found' }
$redist = Join-Path $vs 'VC/Redist/MSVC'
$candidates = foreach($version in Get-ChildItem -LiteralPath $redist -Directory) {
    $x64 = Join-Path $version.FullName 'x64'
    if(Test-Path -LiteralPath $x64) {
        Get-ChildItem -LiteralPath $x64 -Directory | Where-Object {$_.Name -match '^Microsoft\.VC[0-9]+\.CRT$' -and (Test-Path -LiteralPath (Join-Path $_.FullName 'vcruntime140.dll'))}
    }
}
$source = $candidates | Sort-Object { [Version](Get-Item -LiteralPath (Join-Path $_.FullName 'vcruntime140.dll')).VersionInfo.FileVersion } -Descending | Select-Object -First 1 -ExpandProperty FullName
if (!$source) { throw 'x64 Microsoft VC CRT redist directory not found' }
$out = [IO.Path]::GetFullPath($Output)
$bundle = Join-Path $out 'runtime'
if (Test-Path -LiteralPath $bundle) { throw 'Runtime output already exists; use a new output directory' }
New-Item -ItemType Directory -Path $bundle -Force | Out-Null
$files = foreach ($dll in Get-ChildItem -LiteralPath $source -Filter '*.dll' -File) {
    $signature = Get-AuthenticodeSignature -LiteralPath $dll.FullName
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') { throw "Invalid Microsoft runtime signature: $($dll.Name)" }
    $bytes = [IO.File]::ReadAllBytes($dll.FullName)
    $pe = [BitConverter]::ToInt32($bytes, 60)
    if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) { throw 'Runtime DLL is not x64' }
    Copy-Item -LiteralPath $dll.FullName -Destination $bundle
    [ordered]@{ name = $dll.Name; sha256 = (Get-FileHash -LiteralPath $dll.FullName).Hash.ToLowerInvariant(); version = $dll.VersionInfo.FileVersion; signer = $signature.SignerCertificate.Subject }
}
foreach ($required in @('msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll')) {
    if (!($files | Where-Object name -eq $required)) { throw "Missing runtime DLL: $required" }
}
$metadata = [ordered]@{ id = $RuntimeTag; architecture = 'x64'; source_commit = (git rev-parse HEAD).Trim(); packaged_at_utc = [DateTime]::UtcNow.ToString('o'); source = $source; files = @($files) }
$metadata | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $bundle 'runtime.json') -Encoding UTF8
@'
Microsoft Visual C++ Runtime (x64)
Copyright Microsoft Corporation. All rights reserved.
These unmodified redistributable files come from the licensed Visual Studio Microsoft.VC143.CRT redist directory.
They remain subject to the applicable Microsoft Visual Studio software license terms and distributable-code terms.
Redistribution and app-local deployment:
https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
Distributable code list:
https://learn.microsoft.com/visualstudio/releases/2022/redistribution
License terms:
https://visualstudio.microsoft.com/license-terms/
App-local runtime files must be updated with the application; no machine-wide runtime is installed or removed.
'@ | Set-Content -LiteralPath (Join-Path $bundle 'NOTICE.txt') -Encoding UTF8
$zip = Join-Path $out 'Backleaf-runtime-x64.zip'
Compress-Archive -Path (Join-Path $bundle '*') -DestinationPath $zip
Copy-Item -LiteralPath (Join-Path $bundle 'runtime.json') -Destination (Join-Path $out 'runtime.json')
Write-Output "Packaged Microsoft-signed x64 CRT: $zip"
