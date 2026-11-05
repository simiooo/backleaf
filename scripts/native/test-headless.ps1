param([string]$Output='.local/headless.log')
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
Push-Location $projectRoot
try {
    $logPath=[IO.Path]::GetFullPath($Output)
    New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($logPath)) | Out-Null
    cargo test --release --locked -p backleaf 'headless_tests::' -- --test-threads=1 *> $logPath
    $resultCode=$LASTEXITCODE
    $log=Get-Content -LiteralPath $logPath -Raw
    $cases=@([regex]::Matches($log,'(?m)^test (headless_tests::\S+) \.\.\. (ok|FAILED)\s*$') | ForEach-Object {
        [ordered]@{name=$_.Groups[1].Value;passed=$_.Groups[2].Value -eq 'ok'}
    })
    $required=@('async_shell_discovery_and_large_submenu_never_resize_or_move_the_root','dynamic_child_waits_for_rows_and_keeps_both_ancestors_stable','late_dynamic_rows_do_not_revive_closed_columns_or_the_menu_family','menu_family_focus_and_bottom_flip_keep_ancestors_stable')
    $missing=@($required | Where-Object { $name='headless_tests::'+$_; !($cases | Where-Object {$_.name -eq $name -and $_.passed}) })
    $passed=$resultCode -eq 0 -and $cases.Count -ge 20 -and $missing.Count -eq 0 -and @($cases | Where-Object {!$_.passed}).Count -eq 0
    $report=[ordered]@{
        passed=$passed;exit_code=$resultCode;test_count=$cases.Count;cases=$cases;missing_required=$missing
        workspace_commit=(git rev-parse HEAD).Trim();workspace_dirty=[bool](git status --porcelain)
        gpui_revision=(git -C vendor/zed rev-parse HEAD).Trim()
        recorded_at_utc=[DateTime]::UtcNow.ToString('o')
        log_sha256=(Get-FileHash -LiteralPath $logPath).Hash.ToLowerInvariant()
        scope='Production Backleaf view, GPUI TestPlatform/HeadlessAppContext, native event dispatch, real isolated file workers and painted toast geometry. No HWND, desktop input, GPU pixels, screen-reader adapter or native IME/OLE gestures.'
    }
    $reportPath=[IO.Path]::ChangeExtension($logPath,'json')
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $reportPath -Encoding utf8
    Get-Content -LiteralPath $logPath
    if (!$passed) {throw "GPUI headless regressions failed or required menu cases missing (minimum 20): $reportPath"}
    Write-Output "GPUI headless gate passed: $($cases.Count) cases; $reportPath"
} finally {Pop-Location}
