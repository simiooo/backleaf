[CmdletBinding()]
param([Parameter(Mandatory)][string]$InstallDirectory)
$ErrorActionPreference = 'Stop'
if (!('BackleafRuntimeProbe' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class BackleafRuntimeProbe {
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern uint GetModuleFileNameW(IntPtr module, StringBuilder path, int size);
  [DllImport("kernel32.dll")]
  public static extern bool FreeLibrary(IntPtr module);
}
'@
}
$root = [IO.Path]::GetFullPath($InstallDirectory)
$result = foreach ($name in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll')) {
    $path = Join-Path $root $name
    $module = [BackleafRuntimeProbe]::LoadLibraryExW($path, [IntPtr]::Zero, 0x00000100 -bor 0x00000800)
    if ($module -eq [IntPtr]::Zero) { throw "App-local DLL load failed: $name (Win32 $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))" }
    try {
        $buffer = New-Object Text.StringBuilder 32768
        if (![BackleafRuntimeProbe]::GetModuleFileNameW($module, $buffer, $buffer.Capacity)) { throw 'Runtime module path unavailable' }
        if ($buffer.ToString() -ne $path) { throw 'Runtime was resolved from outside the installation' }
        [PSCustomObject]@{ name=$name; loaded_path=$buffer.ToString(); app_local=$true }
    } finally { $null = [BackleafRuntimeProbe]::FreeLibrary($module) }
}
return $result
