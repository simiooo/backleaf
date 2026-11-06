param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Fixture,
    [Parameter(Mandatory=$true)][string]$StateDir,
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
public class Win32Smoke {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
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
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT pt);
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  public struct POINT { public int X; public int Y; }
}
"@
[Win32Smoke]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $OutDir | Out-Null
$shotDir = Join-Path $OutDir 'screenshots'
$uiaDir = Join-Path $OutDir 'uia'
New-Item -ItemType Directory -Force $shotDir | Out-Null
New-Item -ItemType Directory -Force $uiaDir | Out-Null
$script:failures = New-Object System.Collections.ArrayList
$script:steps = New-Object System.Collections.ArrayList
$script:targetPid = 0

function Check([string]$name, [bool]$ok, $detail) {
    $entry = [pscustomobject]@{ name = $name; ok = $ok; detail = $detail; at_utc = (Get-Date).ToUniversalTime().ToString('o') }
    [void]$script:steps.Add($entry)
    if (-not $ok) { [void]$script:failures.Add($name) }
    $tag = 'PASS'
    if (-not $ok) { $tag = 'FAIL' }
    Write-Host ("[{0}] {1} :: {2}" -f $tag, $name, ($detail | Out-String).Trim())
}
function Write-Json($path, $value) {
    $text = $value | ConvertTo-Json -Depth 10
    [IO.File]::WriteAllText($path, $text, (New-Object Text.UTF8Encoding($false)))
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
    [void][Win32Smoke]::GetWindowText($h, $sb, 1024)
    return $sb.ToString()
}
function Get-WindowsForPid([int]$processId) {
    $script:targetPid = $processId
    $list = New-Object System.Collections.ArrayList
    $cb = [Win32Smoke+EnumWindowsProc]{
        param($h, $l)
        $p = 0
        [void][Win32Smoke]::GetWindowThreadProcessId($h, [ref]$p)
        if ($p -eq $script:targetPid -and [Win32Smoke]::IsWindowVisible($h)) {
            $r = New-Object Win32Smoke+RECT
            [void][Win32Smoke]::GetWindowRect($h, [ref]$r)
            $d = New-Object Win32Smoke+RECT
            $dwm = [Win32Smoke]::DwmGetWindowAttribute($h, 9, [ref]$d, 16)
            if ($dwm -ne 0) { $d = $r }
            $title = Get-WindowTitle $h
            [void]$list.Add([pscustomobject]@{
                hwnd = $h.ToInt64(); title = $title;
                rect = @($r.Left, $r.Top, $r.Right, $r.Bottom);
                dwm = @($d.Left, $d.Top, $d.Right, $d.Bottom)
            })
        }
        return $true
    }
    [void][Win32Smoke]::EnumWindows($cb, [IntPtr]::Zero)
    return $list
}
function Set-Foreground([IntPtr]$h) {
    $fg = [Win32Smoke]::GetForegroundWindow()
    $fgPid = 0
    $fgThread = [Win32Smoke]::GetWindowThreadProcessId($fg, [ref]$fgPid)
    $cur = [Win32Smoke]::GetCurrentThreadId()
    [void][Win32Smoke]::AttachThreadInput($cur, $fgThread, $true)
    [void][Win32Smoke]::ShowWindow($h, 9)
    [void][Win32Smoke]::BringWindowToTop($h)
    [void][Win32Smoke]::SetForegroundWindow($h)
    Start-Sleep -Milliseconds 120
    [void][Win32Smoke]::AttachThreadInput($cur, $fgThread, $false)
    return (Wait-Until { [Win32Smoke]::GetForegroundWindow() -eq $h } 1200 100)
}
function Ensure-Foreground([IntPtr]$h) {
    for ($i = 0; $i -lt 4; $i++) {
        if ([Win32Smoke]::GetForegroundWindow() -eq $h) { return $true }
        if (Set-Foreground $h) { return $true }
        Start-Sleep -Milliseconds 200
    }
    return ([Win32Smoke]::GetForegroundWindow() -eq $h)
}
function Save-WindowShot([IntPtr]$h, [string]$name) {
    if (-not (Ensure-Foreground $h)) { throw "window not foreground for screenshot: $name" }
    Start-Sleep -Milliseconds 250
    $d = New-Object Win32Smoke+RECT
    $ok = [Win32Smoke]::DwmGetWindowAttribute($h, 9, [ref]$d, 16)
    if ($ok -ne 0) { [void][Win32Smoke]::GetWindowRect($h, [ref]$d) }
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
    Start-Sleep -Milliseconds 250
}
function Click-Point([int]$x, [int]$y, [IntPtr]$expected) {
    if (-not (Ensure-Foreground $expected)) { throw "refusing to click: target window is not foreground" }
    [void][Win32Smoke]::SetCursorPos($x, $y)
    Start-Sleep -Milliseconds 80
    [Win32Smoke]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [Win32Smoke]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}
function Get-RootByHwnd([int]$processId, [long]$hwnd) {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NativeWindowHandleProperty, [int]$hwnd)
    try { return $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond) } catch { return $null }
}
function Get-AppRoots([int]$processId) {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $processId)
    return @($root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond))
}
function Get-PatternObject($el, $pattern) {
    try {
        $obj = $null
        if ($el.TryGetCurrentPattern($pattern, [ref]$obj)) { return $obj }
    } catch {}
    return $null
}
function Dump-Element($el, [int]$depth, [int]$maxDepth, $acc) {
    if ($null -eq $el -or $acc.Count -ge 700) { return }
    $item = [ordered]@{ depth = $depth }
    try { $item.name = $el.Current.Name } catch { $item.name = '' }
    try { $item.control = $el.Current.ControlType.ProgrammaticName } catch { $item.control = '' }
    try { $item.automationId = $el.Current.AutomationId } catch { $item.automationId = '' }
    try { $r = $el.Current.BoundingRectangle; $item.rect = @([int]$r.X, [int]$r.Y, [int]$r.Width, [int]$r.Height) } catch { $item.rect = $null }
    try { $item.focused = $el.Current.HasKeyboardFocus } catch { $item.focused = $false }
    try { $item.offscreen = $el.Current.IsOffscreen } catch { $item.offscreen = $null }
    $sel = Get-PatternObject $el ([System.Windows.Automation.SelectionItemPattern]::Pattern)
    if ($null -ne $sel) { $item.selected = $sel.Current.IsSelected }
    $range = Get-PatternObject $el ([System.Windows.Automation.RangeValuePattern]::Pattern)
    if ($null -ne $range) { $item.rangeValue = $range.Current.Value; $item.rangeMaximum = $range.Current.Maximum }
    $scroll = Get-PatternObject $el ([System.Windows.Automation.ScrollPattern]::Pattern)
    if ($null -ne $scroll) {
        $item.vertScrollable = $scroll.Current.VerticallyScrollable
        $item.vertPercent = $scroll.Current.VerticalScrollPercent
    }
    [void]$acc.Add([pscustomobject]$item)
    if ($depth -ge $maxDepth) { return }
    try {
        $children = $el.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($c in $children) { Dump-Element $c ($depth + 1) $maxDepth $acc }
    } catch {}
}
function Get-UiaTree($rootEl, [int]$maxDepth = 6) {
    $acc = New-Object System.Collections.ArrayList
    Dump-Element $rootEl 0 $maxDepth $acc
    return @($acc)
}
function Save-UiaDump($rootEl, [string]$name, [int]$maxDepth = 6) {
    $tree = Get-UiaTree $rootEl $maxDepth
    Write-Json (Join-Path $uiaDir ($name + '.json')) $tree
    return $tree
}
function Get-Texts($tree) {
    $texts = New-Object System.Collections.ArrayList
    foreach ($n in $tree) { if ($n.name) { [void]$texts.Add([string]$n.name) } }
    return @($texts)
}
function Find-ByName($rootEl, [string]$name) {
    try {
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
        return $rootEl.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
    } catch { return $null }
}
function Find-ByControlType($rootEl, $controlType) {
    try {
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, $controlType)
        return $rootEl.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
    } catch { return @() }
}
function Get-ScrollbarInfo($rootEl) {
    $bars = @(Find-ByControlType $rootEl ([System.Windows.Automation.ControlType]::ScrollBar))
    if ($bars.Count -eq 0) { return $null }
    $bar = $bars[0]
    $info = [ordered]@{ control = 'ScrollBar' }
    try { $info.name = $bar.Current.Name } catch {}
    try { $r = $bar.Current.BoundingRectangle; $info.rect = @([int]$r.X, [int]$r.Y, [int]$r.Width, [int]$r.Height) } catch {}
    $range = Get-PatternObject $bar ([System.Windows.Automation.RangeValuePattern]::Pattern)
    if ($null -ne $range) { $info.rangeValue = $range.Current.Value; $info.rangeMinimum = $range.Current.Minimum; $info.rangeMaximum = $range.Current.Maximum }
    $scroll = Get-PatternObject $bar ([System.Windows.Automation.ScrollPattern]::Pattern)
    if ($null -ne $scroll) { $info.vertScrollable = $scroll.Current.VerticallyScrollable; $info.vertPercent = $scroll.Current.VerticalScrollPercent }
    return [pscustomobject]$info
}
function Find-MainFileItem($rootEl, [string]$prefix) {
    $items = @(Find-ByControlType $rootEl ([System.Windows.Automation.ControlType]::ListItem))
    foreach ($i in $items) {
        $n = ''
        try { $n = $i.Current.Name } catch {}
        if ($n -and $n.StartsWith($prefix)) { return $i }
    }
    return $null
}
function Find-ExactListItem($rootEl, [string]$name) {
    $items = @(Find-ByControlType $rootEl ([System.Windows.Automation.ControlType]::ListItem))
    foreach ($i in $items) {
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
function Count-Windows([int]$processId) { return @(Get-WindowsForPid $processId).Count }
function Wait-NewWindow([int]$processId, [long]$excludeHwnd, [int]$timeoutMs) {
    $script:foundWindow = $null
    [void](Wait-Until {
        $ws = @(Get-WindowsForPid $processId)
        foreach ($w in $ws) { if ($w.hwnd -ne $excludeHwnd) { $script:foundWindow = $w; return $true } }
        return $false
    } $timeoutMs 150)
    if ($script:foundWindow) { return $script:foundWindow }
    return $null
}

$report = [ordered]@{
    started_utc = (Get-Date).ToUniversalTime().ToString('o')
    exe = $null; exe_sha256 = $null; fixture = $null; session = $null
    desktop = [ordered]@{ locked = $null; virtual_screen = $null }
    main = $null; phases = (New-Object System.Collections.ArrayList)
    screenshots = (New-Object System.Collections.ArrayList)
    uia_text_snapshots = [ordered]@{}
    steps = $script:steps
    failures = $script:failures
    visual_review_required = (New-Object System.Collections.ArrayList)
    passed = $false
    error = $null
    finished_utc = $null
}
$fgBefore = [Win32Smoke]::GetForegroundWindow()
$proc = $null
$props1 = $null
$props2 = $null
$mainHwnd = [IntPtr]::Zero
try {
    $report.exe = (Resolve-Path $Exe).Path
    $report.exe_sha256 = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $report.fixture = (Resolve-Path $Fixture).Path
    $report.desktop.virtual_screen = @([System.Windows.Forms.SystemInformation]::VirtualScreen.Width, [System.Windows.Forms.SystemInformation]::VirtualScreen.Height)
    $sessionFile = Join-Path $OutDir 'session-preflight.json'
    $pre = Start-Process -FilePath $Exe -ArgumentList @('--session-status-file', ('"' + $sessionFile + '"')) -WindowStyle Hidden -PassThru
    [void]$pre.WaitForExit(20000)
    Start-Sleep -Milliseconds 300
    if (Test-Path $sessionFile) { $report.session = Get-Content $sessionFile -Raw | ConvertFrom-Json }
    $report.desktop.locked = $report.session.locked
    Check 'session-unlocked' ($report.session -ne $null -and $report.session.locked -eq $false) $report.session

    $proc = Start-Process -FilePath $Exe -ArgumentList @('--path', ('"' + (Resolve-Path $Fixture).Path + '"'), '--state-dir', ('"' + $StateDir + '"'), '--width', '1120', '--height', '720') -PassThru
    Check 'process-started' ($null -ne $proc) @{ pid = $proc.Id }
    $appeared = Wait-Until { (Count-Windows $proc.Id) -ge 1 } 25000 250
    Check 'main-window-appeared' $appeared @{ pid = $proc.Id }
    $windows = @(Get-WindowsForPid $proc.Id)
    $report.main = $windows | Select-Object -First 1
    $mainHwnd = [IntPtr]$report.main.hwnd
    $rect0 = @($report.main.dwm)
    Check 'main-title' ($report.main.title -like '*Backleaf*') $report.main.title
    $fg = Set-Foreground $mainHwnd
    Check 'main-foreground' $fg @{ hwnd = $mainHwnd.ToInt64() }
    [void]$report.screenshots.Add((Save-WindowShot $mainHwnd '01-main.png'))
    $appRoot = Get-AppRoots $proc.Id | Select-Object -First 1
    $mainTree = Save-UiaDump $appRoot 'main-before'
    $report.uia_text_snapshots.main_before = @(Get-Texts $mainTree)
    Check 'main-uia-root' ($null -ne $appRoot) @{ names = @(Get-Texts $mainTree).Count }

    $filePrefix = 'complete-long-name-'
    $fileItem = Find-MainFileItem $appRoot $filePrefix
    Check 'long-file-item-found' ($null -ne $fileItem) $null
    if ($null -eq $fileItem) { throw 'long file list item not found' }
    $longName = $fileItem.Current.Name
    $fileRect = Get-ElementRect $fileItem
    Click-Point ($fileRect[0] + [int]($fileRect[2] / 2)) ($fileRect[1] + [int]($fileRect[3] / 2)) $mainHwnd
    [void]$report.screenshots.Add((Save-WindowShot $mainHwnd '02-main-file-selected.png'))
    $fileItem2 = Find-MainFileItem $appRoot $filePrefix
    $selInfo = $null
    if ($null -ne $fileItem2) {
        $sp = Get-PatternObject $fileItem2 ([System.Windows.Automation.SelectionItemPattern]::Pattern)
        if ($null -ne $sp) { $selInfo = $sp.Current.IsSelected }
    }
    Check 'file-selection' ($selInfo -eq $true) @{ selectionItemPattern = $selInfo }

    Send-Keys $mainHwnd '%{ENTER}'
    $newWin = Wait-NewWindow $proc.Id $report.main.hwnd 6000
    Check 'properties-window-open' ($null -ne $newWin) $newWin
    if ($null -eq $newWin) { throw 'properties window did not open' }
    $props1 = $newWin
    $p1Hwnd = [IntPtr]$props1.hwnd
    Check 'properties-window-foreground' (Set-Foreground $p1Hwnd) @{ hwnd = $p1Hwnd.ToInt64() }
    $p1Root = Get-RootByHwnd $proc.Id $props1.hwnd
    $detailReady = Wait-Until {
        $tt = @(Get-Texts (Get-UiaTree $p1Root 6))
        return (($tt | Where-Object { $_ -eq '188 B' }).Count -ge 1)
    } 6000 250
    Start-Sleep -Milliseconds 200
    [void]$report.screenshots.Add((Save-WindowShot $p1Hwnd '03-properties-file-open.png'))
    $p1Tree = Save-UiaDump $p1Root 'properties-file-open'
    $p1Texts = @(Get-Texts $p1Tree)
    $report.uia_text_snapshots.properties_file_open = $p1Texts
    $filePath = Join-Path ((Resolve-Path $Fixture).Path) $longName
    Check 'properties-full-name' (@($p1Texts | Where-Object { $_ -eq $longName }).Count -ge 1) @{ expected = $longName }
    Check 'properties-full-path' (@($p1Texts | Where-Object { $_ -eq $filePath }).Count -ge 1) @{ expected = $filePath }
    Check 'properties-file-size-188B' (@($p1Texts | Where-Object { $_ -eq '188 B' }).Count -ge 1) @($p1Texts | Where-Object { $_ -like '*B' })
    $report.visual_review_required.Add('03-properties-file-open.png: summary under size should read 188 字节')
    $mainNow1 = @((Get-WindowsForPid $proc.Id) | Where-Object { $_.hwnd -eq $report.main.hwnd })[0].dwm
    Check 'properties-main-width-unchanged' ((($mainNow1) -join ',') -eq (($rect0) -join ',')) @{ detail_ready = $detailReady; before = $rect0; after = $mainNow1 }
    $report.phases.Add([pscustomobject]@{ phase = 'file-properties-open'; props = $props1; main_dwm = $rect0 })

    Add-Content -LiteralPath $filePath -Value ('x' * 100) -NoNewline -Encoding Ascii
    $newSize = (Get-Item $filePath).Length
    Send-Keys $p1Hwnd '{F5}'
    $f5ok = Wait-Until { $t = @(Get-Texts (Get-UiaTree $p1Root 6)); return (($t | Where-Object { $_ -eq '288 B' }).Count -ge 1) } 6000 250
    [void]$report.screenshots.Add((Save-WindowShot $p1Hwnd '04-properties-file-f5.png'))
    $p1TreeAfter = Save-UiaDump $p1Root 'properties-file-f5'
    $p1TextsAfter = @(Get-Texts $p1TreeAfter)
    $report.uia_text_snapshots.properties_file_f5 = $p1TextsAfter
    Check 'properties-f5-refresh' $f5ok @{ file_bytes = $newSize; texts = @($p1TextsAfter | Where-Object { $_ -like '*B' -or $_ -like '*字节*' }) }

    [void][Win32Smoke]::MoveWindow($p1Hwnd, [int]$props1.dwm[0], [int]$props1.dwm[1], 340, 360, $true)
    Start-Sleep -Milliseconds 800
    $smallRect = @((Get-WindowsForPid $proc.Id) | Where-Object { $_.hwnd -eq $p1Hwnd })[0]
    [void]$report.screenshots.Add((Save-WindowShot $p1Hwnd '05-properties-small-overflow.png'))
    $p1SmallTree = Save-UiaDump $p1Root 'properties-small'
    $bar = Get-ScrollbarInfo $p1Root
    Check 'properties-scrollbar-visible-small' ($null -ne $bar) $bar
    $report.uia_text_snapshots.properties_small = @(Get-Texts $p1SmallTree)
    $report.phases.Add([pscustomobject]@{ phase = 'properties-resized-small'; window = $smallRect; scrollbar = $bar })
    if ($null -ne $bar) {
        $barRect = $bar.rect
        $clickX = $barRect[0] + [int]($barRect[2] / 2)
        $clickY = $barRect[1] + [int]($barRect[3] * 0.82)
        $rangeBefore = $bar.rangeValue
        Click-Point $clickX $clickY $p1Hwnd
        Start-Sleep -Milliseconds 500
        [void]$report.screenshots.Add((Save-WindowShot $p1Hwnd '06-properties-scrolled.png'))
        $barAfter = Get-ScrollbarInfo $p1Root
        $report.uia_text_snapshots.properties_scrolled = @(Get-Texts (Save-UiaDump $p1Root 'properties-scrolled'))
        $moved = $false
        if ($null -ne $barAfter) {
            if ($null -ne $rangeBefore -and $null -ne $barAfter.rangeValue) { $moved = ($barAfter.rangeValue -ne $rangeBefore) }
            elseif ($null -ne $barAfter.vertPercent) { $moved = ($barAfter.vertPercent -gt 0) }
            else { $moved = $true }
        }
        Check 'properties-scrollbar-track-click' $moved @{ before = $rangeBefore; after = $barAfter; click = @($clickX, $clickY) }
        $report.phases.Add([pscustomobject]@{ phase = 'properties-scroll-wheeltrack'; scrollbar_after = $barAfter })
    }

    [void][Win32Smoke]::MoveWindow($p1Hwnd, [int]$props1.dwm[0], [int]$props1.dwm[1], 620, 900, $true)
    Start-Sleep -Milliseconds 900
    [void]$report.screenshots.Add((Save-WindowShot $p1Hwnd '07-properties-large-hidden.png'))
    $p1LargeTree = Save-UiaDump $p1Root 'properties-large'
    $barLarge = Get-ScrollbarInfo $p1Root
    $report.uia_text_snapshots.properties_large = @(Get-Texts $p1LargeTree)
    Check 'properties-scrollbar-hidden-large' ($null -eq $barLarge) @{ scrollbar = $barLarge }
    $report.phases.Add([pscustomobject]@{ phase = 'properties-resized-large'; scrollbar = $barLarge })

    Send-Keys $p1Hwnd '{ESC}'
    $closed1 = Wait-Until { (Count-Windows $proc.Id) -eq 1 } 5000 150
    Check 'properties-escape-close' $closed1 @{ windows = @(Get-WindowsForPid $proc.Id) }
    $mainNow2 = @((Get-WindowsForPid $proc.Id) | Where-Object { $_.hwnd -eq $report.main.hwnd })[0].dwm
    Check 'main-geometry-after-close' ((($mainNow2) -join ',') -eq (($rect0) -join ',')) @{ before = $rect0; after = $mainNow2 }
    $fgMain = Set-Foreground $mainHwnd
    $appRoot = Get-RootByHwnd $proc.Id $report.main.hwnd
    if ($null -eq $appRoot) { $appRoot = @(Get-AppRoots $proc.Id)[0] }
    [void]$report.screenshots.Add((Save-WindowShot $mainHwnd '08-main-after-properties-close.png'))

    $folderItem = Find-ExactListItem $appRoot 'data'
    Check 'folder-item-found' ($null -ne $folderItem) $null
    if ($null -eq $folderItem) { throw 'folder list item not found' }
    $folderRect = Get-ElementRect $folderItem
    Click-Point ($folderRect[0] + [int]($folderRect[2] / 2)) ($folderRect[1] + [int]($folderRect[3] / 2)) $mainHwnd
    Send-Keys $mainHwnd '%{ENTER}'
    $newWin2 = Wait-NewWindow $proc.Id $report.main.hwnd 6000
    Check 'folder-properties-window-open' ($null -ne $newWin2) $newWin2
    if ($null -eq $newWin2) { throw 'folder properties window did not open' }
    $props2 = $newWin2
    $p2Hwnd = [IntPtr]$props2.hwnd
    Check 'folder-properties-foreground' (Set-Foreground $p2Hwnd) @{ hwnd = $p2Hwnd.ToInt64() }
    Start-Sleep -Milliseconds 400
    [void]$report.screenshots.Add((Save-WindowShot $p2Hwnd '09-properties-folder-open.png'))
    $p2Root = Get-RootByHwnd $proc.Id $props2.hwnd
    $countingTree = Save-UiaDump $p2Root 'properties-folder-initial'
    $report.uia_text_snapshots.properties_folder_initial = @(Get-Texts $countingTree)
    Start-Sleep -Milliseconds 1200
    [void]$report.screenshots.Add((Save-WindowShot $p2Hwnd '10-properties-folder-ready.png'))
    $p2Tree = Save-UiaDump $p2Root 'properties-folder-ready'
    $p2Texts = @(Get-Texts $p2Tree)
    $report.uia_text_snapshots.properties_folder_ready = $p2Texts
    Check 'folder-size-8B' (@($p2Texts | Where-Object { $_ -eq '8 B' }).Count -ge 1) @($p2Texts | Where-Object { $_ -like '*B' })
    $report.visual_review_required.Add('10-properties-folder-ready.png: folder summary should read 2 files / 2 folders, 8 B')
    $dataDir = Join-Path ((Resolve-Path $Fixture).Path) 'data'
    [IO.File]::WriteAllText((Join-Path $dataDir 'c.txt'), 'abcd', [Text.Encoding]::ASCII)
    Send-Keys $p2Hwnd '{F5}'
    $f5FolderOk = Wait-Until {
        $t = @(Get-Texts (Get-UiaTree $p2Root 6))
        return (($t | Where-Object { $_ -eq '12 B' }).Count -ge 1)
    } 5000 250
    [void]$report.screenshots.Add((Save-WindowShot $p2Hwnd '11-properties-folder-f5.png'))
    $p2TreeAfter = Save-UiaDump $p2Root 'properties-folder-f5'
    $p2TextsAfter = @(Get-Texts $p2TreeAfter)
    $report.uia_text_snapshots.properties_folder_f5 = $p2TextsAfter
    Check 'folder-f5-12B' $f5FolderOk @($p2TextsAfter | Where-Object { $_ -like '*B' })
    $report.visual_review_required.Add('11-properties-folder-f5.png: folder summary should read 3 files / 2 folders, 12 B')

    Send-Keys $p2Hwnd '{ESC}'
    $closed2 = Wait-Until { (Count-Windows $proc.Id) -eq 1 } 5000 150
    Check 'folder-properties-escape-close' $closed2 @{ windows = @(Get-WindowsForPid $proc.Id) }
    [void](Set-Foreground $mainHwnd)
    [void]$report.screenshots.Add((Save-WindowShot $mainHwnd '12-main-final.png'))

    [void][Win32Smoke]::PostMessage($mainHwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    $exited = $proc.WaitForExit(12000)
    $exitCode = $null
    if ($exited) { $exitCode = $proc.ExitCode }
    Check 'app-clean-exit' $exited @{ exit_code = $exitCode }
    if (-not $exited) { try { $proc.Kill() } catch {} }
    $leftover = 0
    try { $leftover = @(Get-Process Backleaf -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $report.exe }).Count } catch {}
    Check 'no-leftover-process' ($leftover -eq 0) @{ leftover = $leftover }
    $report.passed = ($script:failures.Count -eq 0)
}
catch {
    $report.error = $_.Exception.Message
    try {
        $ws = @(Get-WindowsForPid $proc.Id)
        foreach ($w in $ws) { [void][Win32Smoke]::PostMessage([IntPtr]$w.hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }
        Start-Sleep -Milliseconds 800
        if ($proc -and -not $proc.HasExited) { $proc.Kill() }
    } catch {}
    Write-Host ('ERROR: ' + $_.Exception.Message)
}
finally {
    if ($fgBefore -ne [IntPtr]::Zero) { [void](Set-Foreground $fgBefore) }
    $report.finished_utc = (Get-Date).ToUniversalTime().ToString('o')
    Write-Json (Join-Path $OutDir 'gui-report.json') $report
    Write-Host ('report: ' + (Join-Path $OutDir 'gui-report.json'))
    if ($script:failures.Count -gt 0) { Write-Host ('FAILURES: ' + ($script:failures -join ', ')); exit 1 }
    if ($report.error) { exit 1 }
    Write-Host 'GUI SMOKE PASSED'
    exit 0
}
