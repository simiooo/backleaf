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
    $required=@('properties_scrollbar_tracks_short_window_overflow_and_hides_when_it_fits','file_properties_open_separately_keep_geometry_and_preserve_full_long_path','folder_properties_count_nested_files_without_resizing_or_duplicating_windows','properties_refresh_rechecks_identity_and_close_cancels_the_reader','slow_operations_reveal_once_without_reopening_after_escape','new_folder_in_equivalent_display_path_uses_snapshot_destination_identity','right_click_new_folder_keeps_filename_focused_and_blur_saves_it','late_directory_notification_during_new_folder_locate_preserves_rename_intent','short_successful_operations_do_not_open_or_leave_a_task_panel','indexed_root_case_rename_rebuilds_without_duplicates_and_keeps_nonrecursive_updates','indexed_search_cache_reuse_incremental_refresh_and_replaced_visible_identity','pending_letter_activation_finishes_before_directory_refresh_commits','repeated_letters_cycle_and_wrap_without_intercepting_text_inputs','selected_bytes_include_empty_files_and_never_recurse_into_folders','volume_capacity_updates_keep_sidebar_geometry_and_ignore_stale_or_removed_rows','new_folder_completion_refreshes_in_place_and_enters_rename','directory_refresh_never_clears_the_grid_and_remaps_selection_by_identity','directory_refresh_preserves_scroll_handle_and_visible_anchor','refresh_failure_keeps_old_rows_and_late_results_cannot_replace_navigation','automatic_search_refresh_keeps_rows_until_the_new_page_is_ready','async_shell_discovery_and_large_submenu_never_resize_or_move_the_root','dynamic_child_waits_for_rows_and_keeps_both_ancestors_stable','late_dynamic_rows_do_not_revive_closed_columns_or_the_menu_family','menu_family_focus_and_bottom_flip_keep_ancestors_stable','titlebar_press_dismisses_path_tree_and_search_but_keeps_search_field_presses','automatic_search_uses_covered_index_and_falls_back_after_removal','empty_search_query_does_not_keep_search_surface_open','advanced_filters_require_nonempty_query','finished_non_retryable_operations_hide_badge_and_panel')
    $missing=@($required | Where-Object { $name='headless_tests::'+$_; !($cases | Where-Object {$_.name -eq $name -and $_.passed}) })
    $passed=$resultCode -eq 0 -and $cases.Count -ge 45 -and $missing.Count -eq 0 -and @($cases | Where-Object {!$_.passed}).Count -eq 0
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
    if (!$passed) {throw "GPUI headless regressions failed or required menu/refresh cases missing (minimum 45): $reportPath"}
    Write-Output "GPUI headless gate passed: $($cases.Count) cases; $reportPath"
} finally {Pop-Location}
