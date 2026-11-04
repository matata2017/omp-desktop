# Measure Romp startup and memory for docs/perf.md.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-perf.ps1 [-Tabs 4]
param(
  [int]$Tabs = 4,
  [int]$Samples = 10,
  [switch]$SampleOnly,
  [string]$Label = "state1-idle"
)

$ErrorActionPreference = "Stop"
$exe = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..")).Path "target\debug\romp-desktop.exe"
if (-not (Test-Path $exe)) { throw "build the app first: $exe missing" }

if (-not $SampleOnly) {
Get-Process romp-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2

# --- cold start: launch -> window handle appears -------------------------------------------
$sw = [System.Diagnostics.Stopwatch]::StartNew()
if (-not $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS) {
  # Lets the measurement drive tabs over CDP between samples.
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
}
$proc = Start-Process -FilePath $exe -PassThru
$deadline = (Get-Date).AddSeconds(30)
$visible = $false
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds 25
  $p = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
  if ($p -and $p.MainWindowHandle -ne 0) { $visible = $true; break }
}
$sw.Stop()
if (-not $visible) { throw "window never appeared" }
$coldMs = [math]::Round($sw.Elapsed.TotalMilliseconds)
} else { $coldMs = -1 }

# Every process the app owns: the UI process plus its WebView2 helpers, the agent
# sidecars and the stats child. Walking ParentProcessId keeps unrelated WebView2
# instances out of the number.
function Get-AppTree {
  param([int]$RootPid)
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
  $tree = @($RootPid)
  $added = $true
  while ($added) {
    $added = $false
    foreach ($proc in $all) {
      if ($tree -contains $proc.ParentProcessId -and -not ($tree -contains $proc.ProcessId)) {
        $tree += $proc.ProcessId
        $added = $true
      }
    }
  }
  $tree
}

function Sample-Rss {
  param([string]$State, [int]$Count)
  $values = @()
  $rootPid = (Get-Process romp-desktop -ErrorAction SilentlyContinue | Select-Object -First 1).Id
  for ($i = 0; $i -lt $Count; $i++) {
    $pids = Get-AppTree -RootPid $rootPid
    $rss = (Get-Process -Id $pids -ErrorAction SilentlyContinue | Measure-Object -Property WorkingSet64 -Sum).Sum
    if ($null -ne $rss) { $values += [math]::Round($rss / 1MB, 1) }
    Start-Sleep -Milliseconds 250
  }
  if ($values.Count -eq 0) { return [pscustomobject]@{ state = $State; median = 0; min = 0; max = 0 } }
  $sorted = $values | Sort-Object
  [pscustomobject]@{
    state  = $State
    median = $sorted[[int][math]::Floor($sorted.Count / 2)]
    min    = $sorted[0]
    max    = $sorted[$sorted.Count - 1]
  }
}

# Steady state: wait until the sidecar reports ready (or 20s), otherwise the samples span
# process startup and the numbers are meaningless.
$log = Join-Path $env:APPDATA "dev.romp.desktop\runtime.log"
$settleDeadline = (Get-Date).AddSeconds(20)
$startLines = if (Test-Path $log) { (Get-Content $log | Measure-Object -Line).Lines } else { 0 }
while ((Get-Date) -lt $settleDeadline) {
  Start-Sleep -Milliseconds 300
  if (Test-Path $log) {
    $tail = Get-Content $log | Select-Object -Last 40
    if ($tail -match 'sidecar ready') { break }
  }
}
Start-Sleep -Milliseconds 500

$idle = Sample-Rss -State $Label -Count $Samples

# Composition, so a single number cannot hide which child dominates.
$rootPid = (Get-Process romp-desktop -ErrorAction SilentlyContinue | Select-Object -First 1).Id
$treePids = Get-AppTree -RootPid $rootPid
$detail = Get-CimInstance Win32_Process -Filter "ProcessId=$rootPid" | Select-Object ProcessId, Name
$rows = foreach ($procId in $treePids) {
  $proc = Get-CimInstance Win32_Process -Filter "ProcessId=$procId" -ErrorAction SilentlyContinue
  if (-not $proc) { continue }
  $ws = (Get-Process -Id $procId -ErrorAction SilentlyContinue).WorkingSet64
  [pscustomobject]@{ Role = if ($proc.CommandLine -like '* stats *') { 'stats' } elseif ($proc.CommandLine -like '*rpc-ui*') { 'sidecar' } elseif ($proc.Name -match 'msedgewebview2') { 'webview2' } else { 'app' }; MB = [math]::Round($ws / 1MB, 1) }
}
$rows | Group-Object Role | ForEach-Object {
  Write-Output ("rss_{0}_mb={1}" -f $_.Name, [math]::Round((($_.Group | Measure-Object -Property MB -Sum).Sum), 1))
}
$sidecars = (Get-CimInstance Win32_Process -Filter "Name='omp.exe'" |
  Where-Object { $_.CommandLine -like "*rpc-ui*" } | Measure-Object).Count
$statsChildren = (Get-CimInstance Win32_Process -Filter "Name='omp.exe'" |
  Where-Object { $_.CommandLine -like "*rust-omp*stats*" } | Measure-Object).Count

Write-Output ("cold_start_ms={0}" -f $coldMs)
Write-Output ("rss_state1_median_mb={0}" -f $idle.median)
Write-Output ("rss_state1_min_mb={0}" -f $idle.min)
Write-Output ("rss_state1_max_mb={0}" -f $idle.max)
Write-Output ("sidecars={0} stats_children={1}" -f $sidecars, $statsChildren)
Write-Output ("tabs_requested={0}" -f $Tabs)
