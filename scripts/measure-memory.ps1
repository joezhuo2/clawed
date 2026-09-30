# Sums memory of clawed.exe and every descendant process (msedgewebview2).
# Usage: powershell -File scripts/measure-memory.ps1 [-Label name]
param([string]$Label = "sample")

$all = Get-CimInstance Win32_Process
$root = $all | Where-Object { $_.Name -eq "clawed.exe" } | Select-Object -First 1
if (-not $root) { Write-Output "clawed.exe is not running"; exit 1 }

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
$ws = ($procs | Measure-Object WorkingSet64 -Sum).Sum / 1MB
$priv = ($procs | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB
$backend = Get-Process -Id $root.ProcessId
"{0,-14} processes={1,-2} working_set={2,6:N1} MB private={3,6:N1} MB (backend alone: ws {4:N1} MB, private {5:N1} MB)" -f `
    $Label, $procs.Count, $ws, $priv, ($backend.WorkingSet64 / 1MB), ($backend.PrivateMemorySize64 / 1MB)
