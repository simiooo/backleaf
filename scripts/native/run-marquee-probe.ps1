param(
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][int]$Count,
    [Parameter(Mandatory)][string]$Output,
    [int]$Width=1120,
    [int]$Height=720,
    [int]$Flow=0
)
$ErrorActionPreference='Stop'
$probeExe=(Resolve-Path -LiteralPath $Exe).Path
$probeOutput=[IO.Path]::GetFullPath($Output)
$probeFolder=[IO.Path]::GetDirectoryName($probeOutput)
New-Item -ItemType Directory -Force $probeFolder | Out-Null
$probeSession=[IO.Path]::ChangeExtension($probeOutput,'session.json')
$sessionProcess=Start-Process -FilePath $probeExe -ArgumentList @('--session-status-file',('"'+$probeSession+'"')) -PassThru -Wait -WindowStyle Hidden
if ($sessionProcess.ExitCode) {throw 'Session precheck failed'}
$session=Get-Content -LiteralPath $probeSession -Raw | ConvertFrom-Json
if ($session.locked -or $session.error) {throw 'Desktop locked or session state unknown'}
$probeHash=(Get-FileHash -LiteralPath $probeExe).Hash.ToLowerInvariant()
$probeStarted=Get-Date -Format o
$probeState=Join-Path $probeFolder 'isolated-operations-state'
New-Item -ItemType Directory -Force $probeState | Out-Null
# This is an explicitly visible native performance test, not a background helper.
$probeProcess=Start-Process -FilePath $probeExe -ArgumentList @('--fixture',"$Count",'--flow',"$Flow",'--width',"$Width",'--height',"$Height",'--marquee-smoke','--bench-output',('"'+$probeOutput+'"'),'--state-dir',('"'+$probeState+'"')) -PassThru -WindowStyle Normal
if (!$probeProcess.WaitForExit(60000)) {Stop-Process -Id $probeProcess.Id;throw 'Owned synthetic probe timed out; no pass recorded'}
if ($probeProcess.ExitCode) {throw 'Owned synthetic probe process failed'}
$selection=Get-Content -LiteralPath ([IO.Path]::ChangeExtension($probeOutput,'marquee.json')) -Raw | ConvertFrom-Json
$metrics=Get-Content -LiteralPath $probeOutput -Raw | ConvertFrom-Json
if (!$selection.valid) {throw "Invalid controller run: $($selection.error)"}
if ($metrics.session_initially_locked -ne $false -or $metrics.session_locked_at_end -or $metrics.frames -lt 50) {throw 'Session/frame preconditions failed'}
if ($metrics.max_pending_frame_callbacks -gt 1 -or $metrics.hidden_animation_requests -ne 0 -or $metrics.cached_items_peak -gt 256 -or $metrics.label_cache_peak -gt 256) {throw 'Native cache/clock bound failed'}
if ((Get-FileHash -LiteralPath $probeExe).Hash.ToLowerInvariant() -ne $probeHash) {throw 'Probe executable changed'}
$summary=[ordered]@{exe_sha256=$probeHash;started_at=$probeStarted;finished_at=(Get-Date -Format o);count=$Count;width=$Width;height=$Height;flow=$Flow;controller_valid=$selection.valid;final_identity_pass=$selection.final_identity_pass;selected=$selection.expected_selected;furthest_offset=$selection.furthest_offset;maximum_offset=$selection.maximum_offset;scene_p95_ms=$metrics.scene_cpu.p95_ms;frame_callback_p95_ms=$metrics.frame_callback_interval.p95_ms;working_set_peak_mib=$metrics.peak_working_set_mib;cached_items_peak=$metrics.cached_items_peak;label_cache_peak=$metrics.label_cache_peak;max_pending_frame_callbacks=$metrics.max_pending_frame_callbacks;scope='Visible real GPUI, synthetic controller; no OS input injected'}
$summary | ConvertTo-Json | Set-Content -LiteralPath ([IO.Path]::ChangeExtension($probeOutput,'run.json')) -Encoding utf8
$summary | ConvertTo-Json -Compress
