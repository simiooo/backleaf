param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Fixture,
    [Parameter(Mandatory=$true)][string]$OutDir
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win32Retry {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hwnd, int attr, out RECT rect, int size);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@
[Win32Retry]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $OutDir | Out-Null
$shotDir = Join-Path $OutDir 'screenshots'
New-Item -ItemType Directory -Force $shotDir | Out-Null
$script:failures = New-Object System.Collections.ArrayList
$script:steps = New-Object System.Collections.ArrayList
$script:processes = New-Object System.Collections.ArrayList
$originalClipboard = $null
$restoredClipboard = $false

function Check([string]$name, [bool]$ok, $detail) {
    $entry = [pscustomobject]@{ name = $name; ok = $ok; detail = $detail; at_utc = (Get-Date).ToUniversalTime().ToString('o') }
    [void]$script:steps.Add($entry)
    if (-not $ok) { [void]$script:failures.Add($name) }
    $tag = 'PASS'
    if (-not $ok) { $tag = 'FAIL' }
    Write-Host ("[{0}] {1} :: {2}" -f $tag, $name, (($detail | Out-String).Trim()))
}
function Write-Json($path, $value) { [IO.File]::WriteAllText($path, ($value | ConvertTo-Json -Depth 10), (New-Object Text.UTF8Encoding($false))) }
function Wait-Until([scriptblock]$cond, [int]$timeoutMs, [int]$intervalMs = 200) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ((Get-Date) -lt $deadline) { try { if (& $cond) { return $true } } catch {}; Start-Sleep -Milliseconds $intervalMs }
    return $false
}
function Get-Title([IntPtr]$h) { $sb = New-Object System.Text.StringBuilder 1024; [void][Win32Retry]::GetWindowText($h, $sb, 1024); return $sb.ToString() }
function Get-Class([IntPtr]$h) { $sb = New-Object System.Text.StringBuilder 512; [void][Win32Retry]::GetClassName($h, $sb, 512); return $sb.ToString() }
function Get-Windows([int]$pidFilter = 0) {
    $list = New-Object System.Collections.ArrayList
    $cb = [Win32Retry+EnumWindowsProc]{
        param($h, $l)
        if ([Win32Retry]::IsWindowVisible($h)) {
            $p = 0; [void][Win32Retry]::GetWindowThreadProcessId($h, [ref]$p)
            if ($pidFilter -eq 0 -or $p -eq $pidFilter) {
                $r = New-Object Win32Retry+RECT; [void][Win32Retry]::GetWindowRect($h, [ref]$r)
                $d = New-Object Win32Retry+RECT; $dwm = [Win32Retry]::DwmGetWindowAttribute($h, 9, [ref]$d, 16); if ($dwm -ne 0) { $d = $r }
                [void]$list.Add([pscustomobject]@{ hwnd = $h.ToInt64(); pid = $p; title = (Get-Title $h); class = (Get-Class $h); dwm = @($d.Left, $d.Top, $d.Right, $d.Bottom) })
            }
        }
        return $true
    }
    [void][Win32Retry]::EnumWindows($cb, [IntPtr]::Zero)
    return $list
}
function Set-Foreground([IntPtr]$h) {
    $fg = [Win32Retry]::GetForegroundWindow(); $fgPid = 0
    $fgThread = [Win32Retry]::GetWindowThreadProcessId($fg, [ref]$fgPid); $cur = [Win32Retry]::GetCurrentThreadId()
    [void][Win32Retry]::AttachThreadInput($cur, $fgThread, $true)
    [void][Win32Retry]::ShowWindow($h, 9); [void][Win32Retry]::BringWindowToTop($h); [void][Win32Retry]::SetForegroundWindow($h)
    Start-Sleep -Milliseconds 120; [void][Win32Retry]::AttachThreadInput($cur, $fgThread, $false)
    return (Wait-Until { [Win32Retry]::GetForegroundWindow() -eq $h } 1200 100)
}
function Ensure-Foreground([IntPtr]$h) { for ($i = 0; $i -lt 4; $i++) { if ([Win32Retry]::GetForegroundWindow() -eq $h) { return $true }; if (Set-Foreground $h) { return $true }; Start-Sleep -Milliseconds 200 }; return ([Win32Retry]::GetForegroundWindow() -eq $h) }
function Save-Shot([IntPtr]$h, [string]$name) {
    if (-not (Ensure-Foreground $h)) { throw "window not foreground: $name" }
    Start-Sleep -Milliseconds 250
    $d = New-Object Win32Retry+RECT; $ok = [Win32Retry]::DwmGetWindowAttribute($h, 9, [ref]$d, 16); if ($ok -ne 0) { [void][Win32Retry]::GetWindowRect($h, [ref]$d) }
    $w = $d.Right - $d.Left; $ht = $d.Bottom - $d.Top
    $path = Join-Path $shotDir $name
    $bmp = New-Object System.Drawing.Bitmap $w, $ht; $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($d.Left, $d.Top, 0, 0, (New-Object System.Drawing.Size $w, $ht))
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
    return [pscustomobject]@{ name = $name; path = $path; sha256 = (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant(); rect = @($d.Left, $d.Top, $w, $ht); at_utc = (Get-Date).ToUniversalTime().ToString('o') }
}
function Send-Keys([IntPtr]$h, [string]$keys) { if (-not (Ensure-Foreground $h)) { throw 'target not foreground for keys' }; [System.Windows.Forms.SendKeys]::SendWait($keys); Start-Sleep -Milliseconds 300 }
function Click-Point([int]$x, [int]$y, [IntPtr]$expected) {
    if (-not (Ensure-Foreground $expected)) { throw 'target not foreground for click' }
    [void][Win32Retry]::SetCursorPos($x, $y); Start-Sleep -Milliseconds 80
    [Win32Retry]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero); [Win32Retry]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 400
}
function Get-Root([long]$hwnd) {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NativeWindowHandleProperty, [int]$hwnd)
    try { return $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond) } catch { return $null }
}
function Find-ByNamePrefix($root, [string]$prefix) {
    if ($null -eq $root) { return $null }
    try {
        $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($el in $all) {
            try { $name = $el.Current.Name } catch { continue }
            if ($name -and $name.StartsWith($prefix)) { return $el }
        }
    } catch {}
    return $null
}
function Click-Element($el, [IntPtr]$expected) {
    $r = $el.Current.BoundingRectangle
    Click-Point ([int]($r.X + $r.Width / 2)) ([int]($r.Y + $r.Height / 2)) $expected
}
function Get-UiaTexts([long]$hwnd, [int]$limit = 400) {
    $root = Get-Root $hwnd
    $texts = New-Object System.Collections.ArrayList
    if ($null -eq $root) { return @() }
    try {
        $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $n = 0
        foreach ($el in $all) {
            if ($n -ge $limit) { break }
            $n++
            try { $name = $el.Current.Name } catch { $name = '' }
            try { $control = $el.Current.ControlType.ProgrammaticName } catch { $control = '' }
            try { $r = $el.Current.BoundingRectangle; $rect = @([int]$r.X, [int]$r.Y, [int]$r.Width, [int]$r.Height) } catch { $rect = $null }
            if ($name) { [void]$texts.Add([ordered]@{ name = $name; control = $control; rect = $rect }) }
        }
    } catch {}
    return @($texts)
}
function Get-Clip {
    $state = [ordered]@{ sequence = 0; files = @(); text = $null }
    try {
        $data = [System.Windows.Forms.Clipboard]::GetDataObject()
        if ($data) {
            try { $state.files = @([System.Windows.Forms.Clipboard]::GetFileDropList()) } catch {}
            try { $state.text = [System.Windows.Forms.Clipboard]::GetText() } catch {}
        }
    } catch {}
    return [pscustomobject]$state
}
function Restore-Clipboard {
    if ($script:restoredClipboard) { return }
    try {
        if ($originalClipboard -and @($originalClipboard.files).Count -gt 0) {
            $collection = New-Object System.Collections.Specialized.StringCollection
            foreach ($f in $originalClipboard.files) { [void]$collection.Add([string]$f) }
            [System.Windows.Forms.Clipboard]::SetFileDropList($collection)
            $script:restoredClipboard = $true
        } elseif ($originalClipboard -and $originalClipboard.text) {
            [System.Windows.Forms.Clipboard]::SetText([string]$originalClipboard.text)
            $script:restoredClipboard = $true
        }
    } catch {}
}
function Launch-App([string]$path, [string]$stateDir) {
    $proc = Start-Process -FilePath $script:exePath -ArgumentList @('--path', ('"' + $path + '"'), '--state-dir', ('"' + $stateDir + '"'), '--width', '1000', '--height', '700') -PassThru
    [void]$script:processes.Add($proc)
    $appeared = Wait-Until { @(Get-Windows $proc.Id).Count -ge 1 } 25000 250
    if (-not $appeared) { throw 'app window did not appear' }
    $w = @(Get-Windows $proc.Id)[0]
    return [pscustomobject]@{ proc = $proc; hwnd = [IntPtr]$w.hwnd; window = $w }
}
function Find-ListItems($hwnd) {
    $root = Get-Root $hwnd
    if ($null -eq $root) { return @() }
    try {
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem)
        return @($root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond))
    } catch { return @() }
}
function Close-App($app) {
    if ($null -eq $app) { return }
    try { [void][Win32Retry]::PostMessage($app.hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) } catch {}
    if (-not $app.proc.WaitForExit(10000)) { try { $app.proc.Kill() } catch {} }
}

$report = [ordered]@{
    started_utc = (Get-Date).ToUniversalTime().ToString('o')
    exe = $null; exe_sha256 = $null
    fixture = [ordered]@{}
    steps = $script:steps
    failures = $script:failures
    screenshots = (New-Object System.Collections.ArrayList)
    original_clipboard = $null; restored_clipboard = $false
    passed = $false
    error = $null
    finished_utc = $null
}
$appA = $null
$appB = $null
$lock = $null
try {
    $script:exePath = (Resolve-Path $Exe).Path
    $report.exe = $script:exePath
    $report.exe_sha256 = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $root = (Resolve-Path $Fixture).Path
    $src = Join-Path $root 'src'
    $dest = Join-Path $root 'dest'
    $keep = Join-Path $src 'keep.txt'
    $locked = Join-Path $src 'locked.txt'
    $destKeep = Join-Path $dest 'keep.txt'
    $destLocked = Join-Path $dest 'locked.txt'
    $report.fixture = [ordered]@{ src = $src; dest = $dest; keep = $keep; locked = $locked }
    Get-ChildItem -LiteralPath $dest -Force | Remove-Item -Recurse -Force
    [IO.File]::WriteAllText($keep, "keep original`n", (New-Object Text.UTF8Encoding($false)))
    [IO.File]::WriteAllText($locked, "locked original`n", (New-Object Text.UTF8Encoding($false)))
    $originalClipboard = Get-Clip
    $report.original_clipboard = $originalClipboard

    $appA = Launch-App $src (Join-Path $OutDir 'state-a')
    Check 'app-a-window' ($null -ne $appA) $appA.window
    [void](Ensure-Foreground $appA.hwnd)
    $items = Find-ListItems $appA.hwnd
    Check 'app-a-list-items' ($items.Count -ge 2) @($items | ForEach-Object { $_.Current.Name })
    $r = $items[0].Current.BoundingRectangle
    Click-Point ([int]($r.X + $r.Width / 2)) ([int]($r.Y + $r.Height / 2)) $appA.hwnd
    Send-Keys $appA.hwnd '^a'
    Send-Keys $appA.hwnd '^c'
    $clipReady = Wait-Until { $c = Get-Clip; @($c.files).Count -ge 2 } 15000 250
    Check 'clipboard-copied-two-files' $clipReady (Get-Clip).files
    [void]$report.screenshots.Add((Save-Shot $appA.hwnd '01-source-selected-and-copied.png'))

    $lock = [System.IO.File]::Open($locked, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::None)
    $appB = Launch-App $dest (Join-Path $OutDir 'state-b')
    Check 'app-b-window' ($null -ne $appB) $appB.window
    [void](Ensure-Foreground $appB.hwnd)
    Start-Sleep -Milliseconds 1800
    Send-Keys $appB.hwnd '^v'
    $keepPasted = Wait-Until { Test-Path -LiteralPath $destKeep } 20000 250
    $lockedFailed = Wait-Until { -not (Test-Path -LiteralPath $destLocked) } 1500 200
    Check 'first-paste-copies-unlocked-file' $keepPasted @{ keep = $keepPasted }
    Check 'first-paste-leaves-locked-file-failed' $lockedFailed @{ dest_locked = (Test-Path -LiteralPath $destLocked) }
    Start-Sleep -Milliseconds 2200
    $taskEntry = $null
    [void](Wait-Until { $script:taskEntry = Find-ByNamePrefix (Get-Root $appB.hwnd) '文件操作'; return ($null -ne $script:taskEntry) } 12000 250)
    Check 'task-entry-visible' ($null -ne $script:taskEntry) @{ name = $script:taskEntry.Current.Name }
    [void]$report.screenshots.Add((Save-Shot $appB.hwnd '02-destination-after-failed-paste.png'))
    if ($null -ne $script:taskEntry) {
        Click-Element $script:taskEntry $appB.hwnd
        $retrySeen = Wait-Until { $script:retryButton = Find-ByNamePrefix (Get-Root $appB.hwnd) '重试'; return ($null -ne $script:retryButton) } 5000 200
        Check 'retry-button-visible' $retrySeen @{ name = $script:retryButton.Current.Name }
        Write-Json (Join-Path $OutDir 'task-panel-uia.json') (Get-UiaTexts $appB.hwnd 300)
        [void]$report.screenshots.Add((Save-Shot $appB.hwnd '03-task-panel-with-retry.png'))
        if ($retrySeen) {
            $lock.Dispose()
            $lock = $null
            Click-Element $script:retryButton $appB.hwnd
            $retried = Wait-Until { Test-Path -LiteralPath $destLocked } 20000 250
            $contentOk = $false
            if ($retried) { $contentOk = ((Get-Content -LiteralPath $destLocked -Raw) -eq "locked original`n") }
            Check 'retry-copies-only-failed-item' ($retried -and $contentOk) @{ dest_locked = $retried; content_ok = $contentOk }
            Start-Sleep -Milliseconds 800
            Write-Json (Join-Path $OutDir 'task-panel-after-retry-uia.json') (Get-UiaTexts $appB.hwnd 300)
            [void]$report.screenshots.Add((Save-Shot $appB.hwnd '04-after-retry.png'))
            $destEntries = @(Get-ChildItem -LiteralPath $dest -File | Sort-Object Name | ForEach-Object { $_.Name })
            Check 'destination-has-exactly-two-items' (($destEntries -join ',') -eq 'keep.txt,locked.txt') @($destEntries)
            Check 'keep-not-duplicated' ((Get-Content -LiteralPath $destKeep -Raw) -eq "keep original`n") @{ dest_keep = (Get-Content -LiteralPath $destKeep -Raw) }
        }
    }
    $sourceIntact = ((Get-Content -LiteralPath $keep -Raw) -eq "keep original`n") -and ((Get-Content -LiteralPath $locked -Raw) -eq "locked original`n")
    Check 'sources-intact' $sourceIntact $null
    $report.passed = ($script:failures.Count -eq 0)
}
catch {
    $report.error = $_.Exception.Message
    Write-Host ('ERROR: ' + $_.Exception.Message)
}
finally {
    Write-Host 'FINALLY: report'
    $report.finished_utc = (Get-Date).ToUniversalTime().ToString('o')
    $safeSteps = @($script:steps | ForEach-Object { [ordered]@{ name = [string]$_.name; ok = [bool]$_.ok; detail = [string]($_.detail | Out-String); at_utc = [string]$_.at_utc } })
    $report.steps = $safeSteps
    $report.failures = @($script:failures | ForEach-Object { [string]$_ })
    if ($null -ne $originalClipboard) {
        $report.original_clipboard = [ordered]@{ files = @($originalClipboard.files | ForEach-Object { [string]$_ }); text = [string]$originalClipboard.text }
    }
    Write-Json (Join-Path $OutDir 'retry-report.json') $report
    Write-Host ('report: ' + (Join-Path $OutDir 'retry-report.json'))
    Write-Host 'FINALLY: cleanup'
    if ($lock) { try { $lock.Dispose() } catch {} }
    Close-App $appB
    Close-App $appA
    Write-Host 'FINALLY: clipboard'
    Restore-Clipboard
    $report.restored_clipboard = $script:restoredClipboard
    Write-Host 'FINALLY: done'
    if ($script:failures.Count -gt 0) { Write-Host ('FAILURES: ' + ($script:failures -join ', ')); exit 1 }
    if ($report.error) { exit 1 }
    Write-Host 'RETRY UI SMOKE PASSED'
    exit 0
}
