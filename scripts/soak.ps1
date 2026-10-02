# Soak test: starts a release islet on its own socket, replays the fixture
# in a loop against it and samples memory, to show whether it stays flat over
# hours. Stops the instance at the end.
#
# Usage (from the repo root, after npx tauri build --no-bundle):
#   powershell -File scripts/soak.ps1 -Hours 4 [-SampleMinutes 5] [-Speed 4]
#
# Quit any other islet first: the single-instance guard would hand the launch
# to it. Writes one CSV row per sample (default soak-<timestamp>.csv) and
# prints a summary comparing the start and the end of the run.
param(
    [double]$Hours = 4,
    [double]$SampleMinutes = 5,
    [double]$Speed = 4,
    [string]$Fixture = "fixtures/sample-session.jsonl",
    [string]$App = "target/release/islet.exe",
    [string]$Replay = "target/release/replay.exe",
    [string]$Out = ""
)

foreach ($f in @($App, $Replay, $Fixture)) {
    if (-not (Test-Path $f)) { Write-Output "$f not found; run npx tauri build --no-bundle"; exit 1 }
}
if (Get-Process islet -ErrorAction SilentlyContinue) {
    Write-Output "Another islet.exe is running; quit it first."
    exit 1
}
if (-not $Out) { $Out = "soak-{0:yyyyMMdd-HHmmss}.csv" -f (Get-Date) }

# Its own socket, so real Claude Code sessions never reach the test instance.
$env:ISLET_SOCKET = "islet-soak"
$env:ISLET_NO_AUTOSTART = "1"
$proc = Start-Process -FilePath $App -PassThru
Start-Sleep -Seconds 5

function Measure-Islet {
    $all = Get-CimInstance Win32_Process
    $root = $all | Where-Object { $_.ProcessId -eq $proc.Id }
    if (-not $root) { return $null }
    $ids = @($root.ProcessId)
    $queue = [System.Collections.Queue]::new()
    $queue.Enqueue($root.ProcessId)
    while ($queue.Count -gt 0) {
        $parent = $queue.Dequeue()
        foreach ($child in $all | Where-Object { $_.ParentProcessId -eq $parent }) {
            $ids += $child.ProcessId
            $queue.Enqueue($child.ProcessId)
        }
    }
    $procs = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
    $backend = Get-Process -Id $root.ProcessId
    [pscustomobject]@{
        processes   = $procs.Count
        ws_mb       = [math]::Round(($procs | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
        private_mb  = [math]::Round(($procs | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
        backend_ws_mb      = [math]::Round($backend.WorkingSet64 / 1MB, 1)
        backend_private_mb = [math]::Round($backend.PrivateMemorySize64 / 1MB, 1)
        handles     = $backend.HandleCount
        threads     = $backend.Threads.Count
    }
}

$start = Get-Date
$end = $start.AddHours($Hours)
$nextSample = $start
$loops = 0
$rows = @()
"elapsed_min,loops,processes,ws_mb,private_mb,backend_ws_mb,backend_private_mb,handles,threads" | Set-Content -Encoding utf8 $Out

while ((Get-Date) -lt $end) {
    if ((Get-Date) -ge $nextSample) {
        $m = Measure-Islet
        if (-not $m) { Write-Output "islet.exe exited after $loops loops"; exit 2 }
        $elapsed = [math]::Round(((Get-Date) - $start).TotalMinutes, 1)
        $row = "$elapsed,$loops,$($m.processes),$($m.ws_mb),$($m.private_mb),$($m.backend_ws_mb),$($m.backend_private_mb),$($m.handles),$($m.threads)"
        $row | Add-Content -Encoding utf8 $Out
        $rows += [pscustomobject]@{ elapsed = $elapsed; private = $m.private_mb; backend = $m.backend_private_mb; handles = $m.handles }
        Write-Output $row
        $nextSample = $nextSample.AddMinutes($SampleMinutes)
    }
    & $Replay $Fixture --speed $Speed --fresh-ids | Out-Null
    $loops++
}

if ($rows.Count -ge 2) {
    $span = $rows[-1].elapsed - $rows[0].elapsed
    $window = [math]::Max($SampleMinutes, [math]::Min(60, $span / 4))
    $first = $rows | Where-Object { $_.elapsed -le $rows[0].elapsed + $window }
    $last = $rows | Where-Object { $_.elapsed -ge $rows[-1].elapsed - $window }
    $avg = { param($set, $field) [math]::Round(($set | Measure-Object $field -Average).Average, 1) }
    Write-Output ""
    Write-Output ("{0} loops over {1:N0} min, samples in {2}" -f $loops, $span, $Out)
    Write-Output ("total private:   first {0} MB, last {1} MB" -f (& $avg $first private), (& $avg $last private))
    Write-Output ("backend private: first {0} MB, last {1} MB" -f (& $avg $first backend), (& $avg $last backend))
    Write-Output ("backend handles: first {0}, last {1}" -f (& $avg $first handles), (& $avg $last handles))
}

Stop-Process -Id $proc.Id -ErrorAction SilentlyContinue
