param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Root,
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
public class Win32Clip {
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
  [DllImport("user32.dll")] public static extern uint GetClipboardSequenceNumber();
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@
[Win32Clip]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $OutDir | Out-Null
$shotDir = Join-Path $OutDir 'screenshots'
New-Item -ItemType Directory -Force $shotDir | Out-Null
$script:failures = New-Object System.Collections.ArrayList
$script:steps = New-Object System.Collections.ArrayList
$script:targetPid = 0
$script:diagIndex = 0
$script:closeWindows = New-Object System.Collections.ArrayList
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
function Write-Json($path, $value) {
    [IO.File]::WriteAllText($path, ($value | ConvertTo-Json -Depth 10), (New-Object Text.UTF8Encoding($false)))
}
function Wait-Until([scriptblock]$cond, [int]$timeoutMs, [int]$intervalMs = 200) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ((Get-Date) -lt $deadline) {
        try { if (& $cond) { return $true } } catch {}
        Start-Sleep -Milliseconds $intervalMs
    }
    return $false
}
function Get-WindowTitle([IntPtr]$h) {
    $sb = New-Object System.Text.StringBuilder 1024
    [void][Win32Clip]::GetWindowText($h, $sb, 1024)
    return $sb.ToString()
}
function Get-WindowClass([IntPtr]$h) {
    $sb = New-Object System.Text.StringBuilder 512
    [void][Win32Clip]::GetClassName($h, $sb, 512)
    return $sb.ToString()
}
function Get-AllWindows() {
    $list = New-Object System.Collections.ArrayList
    $cb = [Win32Clip+EnumWindowsProc]{
        param($h, $l)
        if ([Win32Clip]::IsWindowVisible($h)) {
            $p = 0
            [void][Win32Clip]::GetWindowThreadProcessId($h, [ref]$p)
            $r = New-Object Win32Clip+RECT
            [void][Win32Clip]::GetWindowRect($h, [ref]$r)
            $d = New-Object Win32Clip+RECT
            $dwm = [Win32Clip]::DwmGetWindowAttribute($h, 9, [ref]$d, 16)
            if ($dwm -ne 0) { $d = $r }
            [void]$list.Add([pscustomobject]@{
                hwnd = $h.ToInt64(); pid = $p; title = (Get-WindowTitle $h); class = (Get-WindowClass $h);
                rect = @($r.Left, $r.Top, $r.Right, $r.Bottom); dwm = @($d.Left, $d.Top, $d.Right, $d.Bottom)
            })
        }
        return $true
    }
    [void][Win32Clip]::EnumWindows($cb, [IntPtr]::Zero)
    return $list
}
function Get-WindowsForPid([int]$processId) {
    return @(Get-AllWindows | Where-Object { $_.pid -eq $processId })
}
function Set-Foreground([IntPtr]$h) {
    $fg = [Win32Clip]::GetForegroundWindow()
    $fgPid = 0
    $fgThread = [Win32Clip]::GetWindowThreadProcessId($fg, [ref]$fgPid)
    $cur = [Win32Clip]::GetCurrentThreadId()
    [void][Win32Clip]::AttachThreadInput($cur, $fgThread, $true)
    [void][Win32Clip]::ShowWindow($h, 9)
    [void][Win32Clip]::BringWindowToTop($h)
    [void][Win32Clip]::SetForegroundWindow($h)
    Start-Sleep -Milliseconds 120
    [void][Win32Clip]::AttachThreadInput($cur, $fgThread, $false)
    return (Wait-Until { [Win32Clip]::GetForegroundWindow() -eq $h } 1200 100)
}
function Ensure-Foreground([IntPtr]$h) {
    for ($i = 0; $i -lt 4; $i++) {
        if ([Win32Clip]::GetForegroundWindow() -eq $h) { return $true }
        if (Set-Foreground $h) { return $true }
        Start-Sleep -Milliseconds 200
    }
    return ([Win32Clip]::GetForegroundWindow() -eq $h)
}
function Save-WindowShot([IntPtr]$h, [string]$name) {
    if (-not (Ensure-Foreground $h)) { throw "window not foreground for screenshot: $name" }
    Start-Sleep -Milliseconds 250
    $d = New-Object Win32Clip+RECT
    $ok = [Win32Clip]::DwmGetWindowAttribute($h, 9, [ref]$d, 16)
    if ($ok -ne 0) { [void][Win32Clip]::GetWindowRect($h, [ref]$d) }
    $w = $d.Right - $d.Left; $ht = $d.Bottom - $d.Top
    if ($w -le 0 -or $ht -le 0) { throw "bad window rect for $name" }
    $path = Join-Path $shotDir $name
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($d.Left, $d.Top, 0, 0, (New-Object System.Drawing.Size $w, $ht))
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    return [pscustomobject]@{ name = $name; path = $path; sha256 = (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant(); rect = @($d.Left, $d.Top, $w, $ht); at_utc = (Get-Date).ToUniversalTime().ToString('o') }
}
function Send-Keys([IntPtr]$h, [string]$keys) {
    if (-not (Ensure-Foreground $h)) { throw "refusing to send keys: target window is not foreground" }
    [System.Windows.Forms.SendKeys]::SendWait($keys)
    Start-Sleep -Milliseconds 300
}
function Click-Point([int]$x, [int]$y, [IntPtr]$expected) {
    if (-not (Ensure-Foreground $expected)) { throw "refusing to click: target window is not foreground" }
    [void][Win32Clip]::SetCursorPos($x, $y)
    Start-Sleep -Milliseconds 80
    [Win32Clip]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [Win32Clip]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}
function Get-RootByHwnd([long]$hwnd) {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NativeWindowHandleProperty, [int]$hwnd)
    try { return $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond) } catch { return $null }
}
function Get-ListItems($root) {
    try {
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem)
        return @($root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond))
    } catch { return @() }
}
function Find-ListItem($root, [string]$name) {
    foreach ($i in (Get-ListItems $root)) {
        $n = ''
        try { $n = $i.Current.Name } catch {}
        if ($n -eq $name) { return $i }
    }
    return $null
}
function Get-ElementRect($el) {
    $r = $el.Current.BoundingRectangle
    return @([int]$r.X, [int]$r.Y, [int]$r.Width, [int]$r.Height)
}
function Click-Element($el, [IntPtr]$expected) {
    $r = Get-ElementRect $el
    Click-Point ($r[0] + [int]($r[2] / 2)) ($r[1] + [int]($r[3] / 2)) $expected
}
function Get-ClipState {
    $state = [ordered]@{ sequence = [Win32Clip]::GetClipboardSequenceNumber(); formats = @(); files = @(); effect = $null; text = $null; error = $null }
    try {
        $data = [System.Windows.Forms.Clipboard]::GetDataObject()
        if ($data) {
            $state.formats = @($data.GetFormats())
            try { $state.files = @([System.Windows.Forms.Clipboard]::GetFileDropList()) } catch { $state.files = @() }
            try { $state.text = [System.Windows.Forms.Clipboard]::GetText() } catch {}
            try {
                $raw = $data.GetData('Preferred DropEffect')
                $bytes = $null
                if ($raw -is [System.IO.MemoryStream]) { $bytes = $raw.ToArray() }
                elseif ($raw -is [byte[]]) { $bytes = $raw }
                if ($bytes -and $bytes.Length -ge 4) { $state.effect = [BitConverter]::ToUInt32($bytes, 0) }
            } catch {}
        }
    } catch { $state.error = $_.Exception.Message }
    return [pscustomobject]$state
}
function Wait-ClipboardFiles([string[]]$expectedPaths, [int]$timeoutMs, $expectedEffect) {
    $result = $null
    [void](Wait-Until {
        $s = Get-ClipState
        if ($s.error) { return $false }
        $files = @($s.files)
        if ($files.Count -ne $expectedPaths.Count) { return $false }
        $ok = $true
        foreach ($p in $expectedPaths) {
            $match = $files | Where-Object { $_.TrimEnd('\') -ieq $p.TrimEnd('\') }
            if (-not $match) { $ok = $false; break }
        }
        if (-not $ok) { return $false }
        if ($null -ne $expectedEffect -and $s.effect -ne $expectedEffect) { return $false }
        $script:lastClip = $s
        return $true
    } $timeoutMs 150)
    if ($script:lastClip) { return $script:lastClip }
    return $null
}
function Backleaf-ClipboardCommand($app, [string]$keys, [string[]]$expectedPaths, $effect, [int]$attempts = 8) {
    for ($i = 0; $i -lt $attempts; $i++) {
        Send-Keys $app.hwnd $keys
        $s = Wait-ClipboardFiles $expectedPaths 2500 $effect
        if ($null -ne $s) { return $s }
        Start-Sleep -Milliseconds 500
    }
    return $null
}
function Backleaf-PasteUntil($app, [string]$target, [int]$attempts = 10) {
    for ($i = 0; $i -lt $attempts; $i++) {
        Send-Keys $app.hwnd '^v'
        if (Wait-File $target 2500) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return $false
}
function Wait-File([string]$path, [int]$timeoutMs) {
    return (Wait-Until { Test-Path -LiteralPath $path } $timeoutMs 200)
}
function Wait-FileGone([string]$path, [int]$timeoutMs) {
    return (Wait-Until { -not (Test-Path -LiteralPath $path) } $timeoutMs 200)
}
function Get-Sha([string]$path) { return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Get-ExplorerWindows { return @(Get-AllWindows | Where-Object { $_.class -eq 'CabinetWClass' }) }
function Wait-NewExplorerWindow([string]$leaf, [string[]]$exclude, [int]$timeoutMs) {
    $script:foundExplorer = $null
    [void](Wait-Until {
        foreach ($w in (Get-ExplorerWindows)) {
            if ($exclude -contains [string]$w.hwnd) { continue }
            if ($w.title -like ('*' + $leaf + '*')) { $script:foundExplorer = $w; return $true }
        }
        return $false
    } $timeoutMs 250)
    return $script:foundExplorer
}
function Open-Explorer([string]$path, [string]$leaf) {
    $before = @(Get-ExplorerWindows | ForEach-Object { [string]$_.hwnd })
    Start-Process -FilePath 'explorer.exe' -ArgumentList ('"' + $path + '"') | Out-Null
    $w = Wait-NewExplorerWindow $leaf $before 20000
    if ($null -eq $w) { throw "explorer window did not appear for $leaf" }
    [void]$script:closeWindows.Add([pscustomobject]@{ kind = 'explorer'; hwnd = $w.hwnd; title = $w.title })
    return $w
}
function Focus-ExplorerView($w) {
    $h = [IntPtr]$w.hwnd
    if (-not (Ensure-Foreground $h)) { throw 'explorer window not foreground' }
    $x = [int]$w.dwm[0] + [int](($w.dwm[2] - $w.dwm[0]) * 0.5)
    $y = [int]$w.dwm[1] + [int](($w.dwm[3] - $w.dwm[1]) * 0.8)
    Click-Point $x $y $h
    Send-Keys $h '{ESC}'
}
function Launch-Backleaf([string]$path, [string]$stateDir) {
    $proc = Start-Process -FilePath $script:exePath -ArgumentList @('--path', ('"' + $path + '"'), '--state-dir', ('"' + $stateDir + '"'), '--width', '1000', '--height', '700') -PassThru
    [void]$script:processes.Add($proc)
    $diag = New-Object System.Collections.ArrayList
    $appeared = $false
    $deadline = (Get-Date).AddSeconds(25)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 250
        try {
            $ws = @(Get-WindowsForPid $proc.Id)
            if ($ws.Count -ge 1) { $appeared = $true; break }
        } catch {
            [void]$diag.Add([pscustomobject]@{ at = (Get-Date).ToUniversalTime().ToString('o'); error = $_.Exception.Message })
            break
        }
    }
    if (-not $appeared) {
        $all = @()
        try { $all = @(Get-AllWindows | Where-Object { $_.title -like '*Backleaf*' -or $_.title -like '*拾径*' }) } catch { $all = @([pscustomobject]@{ error = $_.Exception.Message }) }
        Write-Json (Join-Path $OutDir ('launch-diagnostic-' + $script:diagIndex + '.json')) ([ordered]@{ pid = $proc.Id; has_exited = $proc.HasExited; errors = $diag; backleaf_windows = $all; all_windows_sample = @(Get-AllWindows | Select-Object -First 10) })
        $script:diagIndex++
        throw 'backleaf window did not appear'
    }
    $w = @(Get-WindowsForPid $proc.Id)[0]
    return [pscustomobject]@{ proc = $proc; hwnd = [IntPtr]$w.hwnd; window = $w }
}
function Select-All-Backleaf($app) {
    $root = Get-RootByHwnd ([long]$app.hwnd)
    $items = Get-ListItems $root
    if ($items.Count -eq 0) { throw 'no file list items in backleaf' }
    Click-Element $items[0] $app.hwnd
    Send-Keys $app.hwnd '^a'
}
function Select-Backleaf-Item($app, [string]$name) {
    $root = Get-RootByHwnd ([long]$app.hwnd)
    $item = Find-ListItem $root $name
    if ($null -eq $item) { throw "backleaf item not found: $name" }
    Click-Element $item $app.hwnd
}
function Close-Window([long]$hwnd) {
    $h = [IntPtr]$hwnd
    if ([Win32Clip]::IsWindowVisible($h)) {
        [void][Win32Clip]::PostMessage($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    }
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

$report = [ordered]@{
    started_utc = (Get-Date).ToUniversalTime().ToString('o')
    exe = $null; exe_sha256 = $null; source_commit = '892eb0e296f6e54ea759aafa3a7021de42c78ad8'
    apartment = [Threading.Thread]::CurrentThread.GetApartmentState().ToString()
    desktop_locked = $null
    original_clipboard = $null; restored_clipboard = $false
    clipboard_states = [ordered]@{}
    fixtures = [ordered]@{}
    steps = $script:steps
    failures = $script:failures
    screenshots = (New-Object System.Collections.ArrayList)
    windows = (New-Object System.Collections.ArrayList)
    passed = $false
    error = $null
    finished_utc = $null
}
try {
    $script:exePath = (Resolve-Path $Exe).Path
    $report.exe = $script:exePath
    $report.exe_sha256 = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $rootPath = (Resolve-Path $Root).Path
    $sessionFile = Join-Path $OutDir 'session-preflight.json'
    $pre = Start-Process -FilePath $Exe -ArgumentList @('--session-status-file', ('"' + $sessionFile + '"')) -WindowStyle Hidden -PassThru
    [void]$pre.WaitForExit(20000)
    Start-Sleep -Milliseconds 200
    $session = $null
    if (Test-Path $sessionFile) { $session = Get-Content $sessionFile -Raw | ConvertFrom-Json }
    $report.desktop_locked = $session.locked
    Check 'session-unlocked' ($session -ne $null -and $session.locked -eq $false) $session
    $originalClipboard = Get-ClipState
    $report.original_clipboard = $originalClipboard
    $src = Join-Path $rootPath 'src'
    $explorerDest = Join-Path $rootPath 'explorer-dest'
    $explorerCutDest = Join-Path $rootPath 'explorer-cut-dest'
    $explorerSrc = Join-Path $rootPath 'explorer-src'
    $backleafPaste = Join-Path $rootPath 'backleaf-paste-dir'
    $srcFiles = @(Get-ChildItem -LiteralPath $src -File | Sort-Object Name)
    $srcPaths = @($srcFiles | ForEach-Object { $_.FullName })
    $srcHashes = [ordered]@{}
    foreach ($f in $srcFiles) { $srcHashes[$f.Name] = Get-Sha $f.FullName }
    $beta = Join-Path $explorerSrc 'beta.txt'
    $betaHash = Get-Sha $beta
    $report.fixtures = [ordered]@{ src = $src; src_files = $srcPaths; src_hashes = $srcHashes; explorer_dest = $explorerDest; explorer_cut_dest = $explorerCutDest; explorer_src = $explorerSrc; beta_sha256 = $betaHash; backleaf_paste_dir = $backleafPaste }
    Check 'fixture-two-source-files' ($srcFiles.Count -eq 2) @($srcFiles.Name)

    $bl1 = Launch-Backleaf $src (Join-Path $OutDir 'state-bl1')
    Check 'bl1-window' ($null -ne $bl1) $bl1.window
    [void]$report.windows.Add($bl1.window)
    Select-All-Backleaf $bl1
    $copyState = Backleaf-ClipboardCommand $bl1 '^c' $srcPaths 1 8
    $report.clipboard_states.after_backleaf_copy = $copyState
    Check 'clipboard-copy-two-paths' ($null -ne $copyState) @{ expected = $srcPaths; actual = $copyState.files; effect = $copyState.effect }
    $unchanged = $true
    foreach ($f in $srcFiles) { if ((Get-Sha $f.FullName) -ne $srcHashes[$f.Name]) { $unchanged = $false } }
    Check 'backleaf-copy-keeps-source' ($unchanged -and $srcFiles.Count -eq @(Get-ChildItem -LiteralPath $src -File).Count) $null
    [void]$report.screenshots.Add((Save-WindowShot $bl1.hwnd '01-backleaf-after-copy.png'))

    $ex1 = Open-Explorer $explorerDest 'explorer-dest'
    [void]$report.windows.Add($ex1)
    Focus-ExplorerView $ex1
    Send-Keys ([IntPtr]$ex1.hwnd) '^v'
    $pastedAll = $true
    foreach ($f in $srcFiles) {
        $target = Join-Path $explorerDest $f.Name
        if (-not (Wait-File $target 20000)) { $pastedAll = $false; break }
    }
    $hashOk = $true
    foreach ($f in $srcFiles) {
        $target = Join-Path $explorerDest $f.Name
        if ((Test-Path -LiteralPath $target) -and (Get-Sha $target) -ne $srcHashes[$f.Name]) { $hashOk = $false }
    }
    Check 'explorer-paste-copy-both-files' ($pastedAll -and $hashOk) @{ dest = $explorerDest; files = @(Get-ChildItem -LiteralPath $explorerDest -File | ForEach-Object { $_.Name }) }
    [void]$report.screenshots.Add((Save-WindowShot ([IntPtr]$ex1.hwnd) '02-explorer-after-paste-copy.png'))

    [void](Ensure-Foreground $bl1.hwnd)
    Select-Backleaf-Item $bl1 'alpha.txt'
    $cutState = Backleaf-ClipboardCommand $bl1 '^x' @((Join-Path $src 'alpha.txt')) 2 8
    $report.clipboard_states.after_backleaf_cut = $cutState
    Check 'clipboard-cut-one-path-move-effect' ($null -ne $cutState) @{ actual = $cutState.files; effect = $cutState.effect }
    [void]$report.screenshots.Add((Save-WindowShot $bl1.hwnd '03-backleaf-after-cut.png'))

    $ex2 = Open-Explorer $explorerCutDest 'explorer-cut-dest'
    [void]$report.windows.Add($ex2)
    Focus-ExplorerView $ex2
    Send-Keys ([IntPtr]$ex2.hwnd) '^v'
    $cutPasted = Wait-File (Join-Path $explorerCutDest 'alpha.txt') 20000
    $cutSourceGone = Wait-FileGone (Join-Path $src 'alpha.txt') 20000
    $cutHashOk = $false
    if ($cutPasted) { $cutHashOk = ((Get-Sha (Join-Path $explorerCutDest 'alpha.txt')) -eq $srcHashes['alpha.txt']) }
    Check 'explorer-paste-cut-moves-file' ($cutPasted -and $cutSourceGone -and $cutHashOk) @{ dest_exists = $cutPasted; source_gone = $cutSourceGone; hash_ok = $cutHashOk }
    [void]$report.screenshots.Add((Save-WindowShot ([IntPtr]$ex2.hwnd) '04-explorer-after-paste-cut.png'))

    $bl2 = Launch-Backleaf $backleafPaste (Join-Path $OutDir 'state-bl2')
    Check 'bl2-window' ($null -ne $bl2) $bl2.window
    [void]$report.windows.Add($bl2.window)
    Start-Sleep -Milliseconds 1500
    $ex3 = Open-Explorer $explorerSrc 'explorer-src'
    [void]$report.windows.Add($ex3)
    $ex3h = [IntPtr]$ex3.hwnd
    [void](Ensure-Foreground $ex3h)
    $rootEx = Get-RootByHwnd ([long]$ex3.hwnd)
    $betaItem = Find-ListItem $rootEx 'beta.txt'
    if ($null -ne $betaItem) { Click-Element $betaItem $ex3h } else { Focus-ExplorerView $ex3; Send-Keys $ex3h 'beta' }
    Send-Keys $ex3h '^c'
    $betaClip = Wait-ClipboardFiles @($beta) 15000 $null
    $report.clipboard_states.after_explorer_copy = $betaClip
    Check 'clipboard-explorer-copy-beta' ($null -ne $betaClip) @{ actual = $betaClip.files; effect = $betaClip.effect; uia_item = ($null -ne $betaItem) }
    [void]$report.screenshots.Add((Save-WindowShot $ex3h '05-explorer-after-copy-beta.png'))

    $betaPasted = Backleaf-PasteUntil $bl2 (Join-Path $backleafPaste 'beta.txt') 10
    $betaHashOk = $false
    if ($betaPasted) { $betaHashOk = ((Get-Sha (Join-Path $backleafPaste 'beta.txt')) -eq $betaHash) }
    Check 'backleaf-paste-from-explorer' ($betaPasted -and $betaHashOk) @{ exists = $betaPasted; hash_ok = $betaHashOk; dir = $backleafPaste }
    [void]$report.screenshots.Add((Save-WindowShot $bl2.hwnd '06-backleaf-after-paste-beta.png'))

    [void](Ensure-Foreground $bl1.hwnd)
    $remainingEntry = @(Get-ChildItem -LiteralPath $src -File | Select-Object -First 1)
    $remainingUnicode = $remainingEntry.Name
    Select-Backleaf-Item $bl1 $remainingUnicode
    $cut2 = Backleaf-ClipboardCommand $bl1 '^x' @($remainingEntry.FullName) 2 8
    $report.clipboard_states.cut_before_external_change = $cut2
    Check 'second-cut-before-external-change' ($null -ne $cut2) @{ effect = $cut2.effect }
    [void]$report.screenshots.Add((Save-WindowShot $bl1.hwnd '07-backleaf-cut-before-external-change.png'))
    [System.Windows.Forms.Clipboard]::SetText('backleaf clipboard interop external change marker')
    Start-Sleep -Milliseconds 1500
    $external = Get-ClipState
    $report.clipboard_states.after_external_text_change = $external
    Check 'external-text-clipboard-applied' ($external.text -eq 'backleaf clipboard interop external change marker') @{ text = $external.text; effect = $external.effect }
    [void]$report.screenshots.Add((Save-WindowShot $bl1.hwnd '08-backleaf-after-external-change.png'))
    Check 'bl1-still-running' (-not $bl1.proc.HasExited) @{ pid = $bl1.proc.Id }

    Close-Window $bl1.hwnd
    Close-Window $bl2.hwnd
    $bl1Closed = $bl1.proc.WaitForExit(15000)
    $bl2Closed = $bl2.proc.WaitForExit(15000)
    Check 'backleaf-clean-exit' ($bl1Closed -and $bl2Closed) @{ bl1_exit = $bl1.proc.ExitCode; bl2_exit = $bl2.proc.ExitCode }
    if (-not $bl1Closed) { try { $bl1.proc.Kill() } catch {} }
    if (-not $bl2Closed) { try { $bl2.proc.Kill() } catch {} }
    $leftover = 0
    try { $leftover = @(Get-Process Backleaf -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $script:exePath }).Count } catch {}
    Check 'no-leftover-backleaf' ($leftover -eq 0) @{ leftover = $leftover }
    $report.passed = ($script:failures.Count -eq 0)
}
catch {
    $report.error = $_.Exception.Message
    Write-Host ('ERROR: ' + $_.Exception.Message)
}
finally {
    Restore-Clipboard
    $report.restored_clipboard = $script:restoredClipboard
    foreach ($entry in $script:closeWindows) {
        try { Close-Window ([long]$entry.hwnd) } catch {}
    }
    Start-Sleep -Milliseconds 500
    foreach ($p in $script:processes) {
        try { if (-not $p.HasExited) { [void]$p.WaitForExit(3000); if (-not $p.HasExited) { $p.Kill() } } } catch {}
    }
    $report.finished_utc = (Get-Date).ToUniversalTime().ToString('o')
    Write-Json (Join-Path $OutDir 'clipboard-report.json') $report
    Write-Host ('report: ' + (Join-Path $OutDir 'clipboard-report.json'))
    if ($script:failures.Count -gt 0) { Write-Host ('FAILURES: ' + ($script:failures -join ', ')); exit 1 }
    if ($report.error) { exit 1 }
    Write-Host 'CLIPBOARD INTEROP SMOKE PASSED'
    exit 0
}
