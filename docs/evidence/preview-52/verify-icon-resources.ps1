param([Parameter(Mandatory)][string]$Binary,[string]$Output='.local/logo-v3-pe.json')
$ErrorActionPreference='Stop'
if (-not ('BrandResourceRead' -as [type])) {
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class BrandResourceRead {
 [DllImport("kernel32",CharSet=CharSet.Unicode,SetLastError=true)] public static extern IntPtr LoadLibraryExW(string path,IntPtr file,uint flags);
 [DllImport("kernel32",SetLastError=true)] public static extern IntPtr FindResourceW(IntPtr module,IntPtr name,IntPtr type);
 [DllImport("kernel32",SetLastError=true)] public static extern IntPtr LoadResource(IntPtr module,IntPtr resource);
 [DllImport("kernel32",SetLastError=true)] public static extern IntPtr LockResource(IntPtr resource);
 [DllImport("kernel32",SetLastError=true)] public static extern uint SizeofResource(IntPtr module,IntPtr resource);
 [DllImport("kernel32")] public static extern bool FreeLibrary(IntPtr module);
 public static byte[] Read(IntPtr module,int id,int type) {
  var info=FindResourceW(module,new IntPtr(id),new IntPtr(type));if(info==IntPtr.Zero)throw new Exception("Missing resource");
  var length=SizeofResource(module,info);var data=LockResource(LoadResource(module,info));var bytes=new byte[length];Marshal.Copy(data,bytes,0,(int)length);return bytes;
 }
}
'@
}
$path=(Resolve-Path -LiteralPath $Binary).Path
$module=[BrandResourceRead]::LoadLibraryExW($path,[IntPtr]::Zero,2)
if($module -eq [IntPtr]::Zero){throw 'Cannot inspect PE resources'}
try{
 $group=[BrandResourceRead]::Read($module,1,14);$count=[BitConverter]::ToUInt16($group,4)
 if($count -ne 9){throw 'Expected nine frames at resource group ID1'}
 $frames=@()
 for($i=0;$i -lt $count;$i++){
  $at=6+14*$i;$size=[int]$group[$at];if(!$size){$size=256};$id=[BitConverter]::ToUInt16($group,$at+12)
  $bytes=[BrandResourceRead]::Read($module,$id,3)
  $hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
  $expected=(Get-FileHash "assets/brand/icon-v3/$size.png").Hash.ToLowerInvariant()
  if($hash -ne $expected){throw "Embedded $size px icon differs from the new generated asset"}
  $frames+=@{size=$size;resource_id=$id;sha256=$hash}
 }
 @{passed=$true;binary=$path;exe_sha256=(Get-FileHash $path).Hash.ToLowerInvariant();group=1;frames=$frames;scope='Read-only PE resources, no code execution or window automation'}|ConvertTo-Json -Depth 4|Set-Content $Output -Encoding utf8
 Write-Output 'PE group ID1 contains all nine exact new icon PNGs'
}finally{[void][BrandResourceRead]::FreeLibrary($module)}

