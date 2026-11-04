param([Parameter(Mandatory)][string]$Binary,[string]$Output="$PSScriptRoot/../../.local/icon-performance",[int]$Seconds=12)
$ErrorActionPreference='Stop'
$binaryPath=(Resolve-Path -LiteralPath $Binary).Path
New-Item -ItemType Directory -Force $Output | Out-Null
$outputPath=(Resolve-Path -LiteralPath $Output).Path
$build=(Get-FileHash -LiteralPath $binaryPath).Hash.ToLowerInvariant()
$fixture=Join-Path $outputPath ('owned-exe-'+$build.Substring(0,8))
New-Item -ItemType Directory -Force $fixture | Out-Null
# These are exclusively generated fixtures. Hard links exercise the real PE
# resource extractor without storing a thousand copies or executing any file.
for($i=0;$i -lt 1024;$i++) {
    $file=Join-Path $fixture ('fixture-{0:D4}.exe' -f $i)
    if(Test-Path -LiteralPath $file){continue}
    if($i%4 -eq 0){[IO.File]::WriteAllText($file,'owned fixture without embedded resources')}
    else{New-Item -ItemType HardLink -Path $file -Target $binaryPath | Out-Null}
}
$summary=@()
foreach($case in @(@{name='1024-real-exe-scroll';args=@('--path',$fixture,'--flow','0')},@{name='10m-scroll-flow8';args=@('--fixture','10000000','--flow','8')})) {
    $session=Join-Path $outputPath 'session.json'
    $preflight=Start-Process -FilePath $binaryPath -ArgumentList @('--session-status-file',('"'+$session+'"')) -PassThru -Wait -WindowStyle Hidden
    if($preflight.ExitCode -ne 0){throw 'Session preflight failed'}
    $state=Get-Content -LiteralPath $session -Raw | ConvertFrom-Json
    if($state.locked -or $state.error){throw 'Desktop locked; visible benchmark deferred'}
    $report=Join-Path $outputPath ($case.name+'.json')
    $arguments=@($case.args)+@('--scroll','--bench-output',$report,'--seconds',"$Seconds",'--state-dir',(Join-Path $outputPath 'state'))
    $quoted=$arguments | ForEach-Object {'"'+$_+'"'}
    $started=(Get-Date).ToString('o')
    $process=Start-Process -FilePath $binaryPath -ArgumentList $quoted -PassThru -Wait -WindowStyle Normal -RedirectStandardError (Join-Path $outputPath ($case.name+'.stderr.log'))
    if($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $report)){throw 'Native benchmark did not produce a report'}
    $r=Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if($r.session_initially_locked -ne $false -or $r.session_locked_at_end -or ($r.events | Where-Object {$_ -match 'session-locked'})){throw 'Desktop locked during native benchmark'}
    if($r.application_icons.cache_peak -gt 128 -or $r.application_icons.queue_peak -gt 32 -or $r.application_icons.result_peak -gt 32){throw 'Native icon bound exceeded'}
    if($case.name -eq '1024-real-exe-scroll' -and ($r.application_icons.extracted -le 128 -or $r.application_icons.cache_peak -ne 128)){throw 'Real icon cache eviction was not exercised'}
    $summary+=@{case=$case.name;source_exe_sha256=$build;started_at=$started;report_sha256=(Get-FileHash -LiteralPath $report).Hash.ToLowerInvariant();scene_p95_ms=$r.scene_cpu.p95_ms;frame_callback_p95_ms=$r.frame_callback_interval.p95_ms;working_set_peak_mib=$r.peak_working_set_mib;visible_cells_peak=$r.visible_cells_peak;cached_items_peak=$r.cached_items_peak;application_icons=$r.application_icons}
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $outputPath 'summary.json') -Encoding utf8
$summary | ConvertTo-Json -Depth 6
