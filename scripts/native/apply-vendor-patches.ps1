$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$file = Join-Path $root 'vendor/zed/crates/gpui_windows/src/direct_write.rs'
if (!(Test-Path -LiteralPath $file)) { throw "Missing vendored DirectWrite source: $file" }
$marker = '// Backleaf: no synchronous font-cache update check'
$text = [IO.File]::ReadAllText($file)
if ($text.Contains($marker)) {
    Write-Host 'Vendored DirectWrite startup patch already applied'
    exit 0
}
$old = '.GetSystemFontCollection(false, &mut result, true)?;'
$new = '.GetSystemFontCollection(false, &mut result, false)?; ' + $marker
if (!$text.Contains($old)) { throw 'DirectWrite startup patch target not found' }
$text = $text.Replace($old, $new)
[IO.File]::WriteAllText($file, $text, (New-Object System.Text.UTF8Encoding($false)))
if (!([IO.File]::ReadAllText($file)).Contains($marker)) {
    throw 'DirectWrite startup patch verification failed'
}
Write-Host 'Applied vendored DirectWrite startup patch'
