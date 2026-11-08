# Measure one build: cold start, boot milestones, and process-tree footprint.
# Usage: powershell -NoProfile -File scripts/perf/measure.ps1 -Exe <path> -Label <name> [-Tabs 2]
# Writes one JSON object to stdout (and to -Out when given); human summary goes to stderr.
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [Parameter(Mandatory = $true)][string]$Label,
  [int]$Tabs = 1,
  [int]$Port = 9222,
  [int]$ReadyTimeoutSeconds = 30,
  [int]$SettleSeconds = 2,
  [int]$Samples = 10,
  [switch]$NoReady,
  [switch]$NoClean,
  [switch]$NoDrive,
  [switch]$KeepRunning,
  [string]$WorkingDirectory,
  [string[]]$Arguments = @(),
  [string]$LogPath,
  [string]$Out
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $Exe)) { throw "exe not found: $Exe" }
$exePath = (Resolve-Path $Exe).Path
$exeItem = Get-Item $exePath

if (-not $LogPath) {
  $dataDir = if ($env:OMP_DATA_DIR) { $env:OMP_DATA_DIR } else { Join-Path $env:APPDATA "dev.romp.desktop" }
  $LogPath = Join-Path $dataDir "runtime.log"
}

function Get-Tree {
  param([int]$RootPid)
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
  $tree = @($RootPid)
  $added = $true
  while ($added) {
    $added = $false
    foreach ($proc in $all) {
      if (($tree -contains $proc.ParentProcessId) -and -not ($tree -contains $proc.ProcessId)) {
        $tree += $proc.ProcessId
        $added = $true
      }
    }
  }
  $tree
}

function Role-Of {
  param([string]$Name, [string]$CommandLine)
  if ($CommandLine -like "* stats *") { return "stats" }
  if ($CommandLine -like "*rpc-ui*") { return "sidecar" }
  if ($Name -like "msedgewebview2*") { return "webview2" }
  if ($Name -match "^(chrome_crashpad|crashpad)") { return "crashpad" }
  # Electron helpers carry --type=; its browser process does not.
  if ($CommandLine -like "*--type=*") { return "webview2" }
  if ($Name -like "*omp*") { return "app" }
  if ($Name -match "^(electron|omp-gui)") { return "app" }
  return "helper"
}

# A previously running instance would make the launch a no-op (single-instance lock). The lock
# is per app identifier, so a debug build blocks a release build of the same app; kill by name
# for that app only. Other targets (for example electron.exe) are killed by exact path so an
# unrelated app is never touched.
$killByName = $exeItem.Name -eq "romp-desktop.exe"
if (-not $NoClean) {
  Get-CimInstance Win32_Process |
    Where-Object {
      $_.ExecutablePath -eq $exePath -or ($killByName -and $_.Name -eq $exeItem.Name)
    } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
}

$prevWebViewArgs = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$prevMaxSidecars = $env:ROMP_MAX_SIDECARS
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
if ($Tabs -gt 3 -and -not $env:ROMP_MAX_SIDECARS) { $env:ROMP_MAX_SIDECARS = "$Tabs" }

$launchedPid = $null
try {
  $startEpoch = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $argumentList = @($Arguments)
  if ($WorkingDirectory -and $argumentList.Count -gt 0) {
    $proc = Start-Process -FilePath $exePath -WorkingDirectory $WorkingDirectory -ArgumentList $argumentList -PassThru
  } elseif ($WorkingDirectory) {
    $proc = Start-Process -FilePath $exePath -WorkingDirectory $WorkingDirectory -PassThru
  } elseif ($argumentList.Count -gt 0) {
    $proc = Start-Process -FilePath $exePath -ArgumentList $argumentList -PassThru
  } else {
    $proc = Start-Process -FilePath $exePath -PassThru
  }
  $launchedPid = $proc.Id
  $deadline = (Get-Date).AddSeconds(60)
  $visible = $false
  while ((Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 50
    foreach ($procId in (Get-Tree -RootPid $proc.Id)) {
      $child = Get-Process -Id $procId -ErrorAction SilentlyContinue
      if ($child -and $child.MainWindowHandle -ne 0) { $visible = $true; break }
    }
    if ($visible) { break }
    if ($proc.HasExited -and -not (Get-Process -Id $proc.Id -ErrorAction SilentlyContinue)) { break }
  }
  $sw.Stop()
  if (-not $visible) { throw "no visible window within 60s for $Label" }
  $coldMs = [math]::Round($sw.Elapsed.TotalMilliseconds)

  # The drive step opens the extra tabs and waits for their sidecars. Running it even for one
  # tab collects the renderer marks. JSON goes through a file: a native command's stdout and
  # stderr share one pipeline here, and an interleaved warning would corrupt the parse.
  $drive = $null
  $driveError = $null
  if ($NoDrive) {
    $driveError = "skipped: -NoDrive"
  } else {
    $driveOut = Join-Path ([System.IO.Path]::GetTempPath()) ("romp-drive-" + [guid]::NewGuid().ToString("N") + ".json")
  }
  if (-not $NoDrive) {
    try {
      $driveScript = Join-Path $PSScriptRoot "drive.mjs"
      $driveRaw = & node $driveScript --port $Port --open $Tabs --wait-ready ($ReadyTimeoutSeconds * 1000) --out $driveOut 2>&1
      if (Test-Path $driveOut) {
        $drive = Get-Content $driveOut -Raw | ConvertFrom-Json
        Remove-Item $driveOut -Force -ErrorAction SilentlyContinue
      } else {
        $driveError = ($driveRaw | Out-String).Trim()
        [Console]::Error.WriteLine("drive failed: $driveError")
      }
    } catch {
      $driveError = $_.Exception.Message
      [Console]::Error.WriteLine("drive failed: $driveError")
    }
  }

  # Ready gate: count sidecar-ready lines from this run only. The epoch field is written by
  # the app; older lines are filtered out so a previous run cannot satisfy the gate.
  $readyEpochs = @()
  $readyTimedOut = $false
  if (-not $NoReady) {
    $readyDeadline = (Get-Date).AddSeconds($ReadyTimeoutSeconds)
    while ((Get-Date) -lt $readyDeadline) {
      if (Test-Path $LogPath) {
        $readyEpochs = @(
          Get-Content $LogPath -Tail 400 |
            Where-Object { $_ -match "sidecar ready" -and $_ -match "epoch_ms=(\d+)" } |
            ForEach-Object { [int64]$Matches[1] } |
            Where-Object { $_ -ge ($startEpoch - 2000) }
        )
        if ($readyEpochs.Count -ge $Tabs) { break }
      }
      Start-Sleep -Milliseconds 200
    }
    if ($readyEpochs.Count -lt $Tabs) { $readyTimedOut = $true }
  }
  Start-Sleep -Seconds $SettleSeconds

  $rootPid = $proc.Id
  $tree = Get-Tree -RootPid $rootPid
  $values = @()
  for ($i = 0; $i -lt $Samples; $i++) {
    $tree = Get-Tree -RootPid $rootPid
    $sum = (Get-Process -Id $tree -ErrorAction SilentlyContinue | Measure-Object -Property WorkingSet64 -Sum).Sum
    if ($null -ne $sum) { $values += [math]::Round($sum / 1MB, 1) }
    Start-Sleep -Milliseconds 250
  }
  $sorted = @($values | Sort-Object)
  $median = if ($sorted.Count -gt 0) { $sorted[[int][math]::Floor($sorted.Count / 2)] } else { 0 }

  $rows = foreach ($procId in (Get-Tree -RootPid $rootPid)) {
    $info = Get-CimInstance Win32_Process -Filter "ProcessId=$procId" -ErrorAction SilentlyContinue
    if (-not $info) { continue }
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    $handles = 0
    $threads = 0
    if ($p) {
      $handles = $p.HandleCount
      $threads = $p.Threads.Count
    }
    [pscustomobject]@{
      Role    = Role-Of -Name $info.Name -CommandLine ([string]$info.CommandLine)
      MB      = if ($p) { [math]::Round($p.WorkingSet64 / 1MB, 1) } else { 0 }
      Handles = $handles
      Threads = $threads
    }
  }

  $breakdown = @(
    $rows | Group-Object Role | ForEach-Object {
      [pscustomobject]@{
        role      = $_.Name
        mb        = [math]::Round((($_.Group | Measure-Object -Property MB -Sum).Sum), 1)
        processes = $_.Count
        handles   = (($_.Group | Measure-Object -Property Handles -Sum).Sum)
        threads   = (($_.Group | Measure-Object -Property Threads -Sum).Sum)
      }
    } | Sort-Object mb -Descending
  )

  $marks = @{}
  if ($drive -and $drive.marks) {
    foreach ($mark in $drive.marks) {
      # HMR can re-run the renderer; the first occurrence is the real boot.
      if (-not $marks.ContainsKey($mark.name)) { $marks[$mark.name] = [int64]$mark.epochMs }
    }
  }
  $firstPaint = if ($marks.ContainsKey("boot:first-paint")) { $marks["boot:first-paint"] } else { $null }
  $firstSession = if ($marks.ContainsKey("chat:first-session-rendered")) { $marks["chat:first-session-rendered"] } else { $null }
  $firstReady = if ($readyEpochs.Count -gt 0) { ($readyEpochs | Measure-Object -Minimum).Minimum } else { $null }
  $tabStates = @()
  if ($drive -and $drive.tabs) { $tabStates = @($drive.tabs) }

  $result = [ordered]@{
    label           = $Label
    exe             = $exeItem.Name
    exePath         = $exePath
    builtAt         = $exeItem.LastWriteTime.ToString("o")
    startedAt       = [DateTimeOffset]::FromUnixTimeMilliseconds($startEpoch).ToString("o")
    coldStartMs     = $coldMs
    milestones      = [ordered]@{
      windowMs       = $coldMs
      webviewMs      = if ($firstPaint) { $firstPaint - $startEpoch } else { $null }
      sidecarReadyMs = if ($firstReady) { $firstReady - $startEpoch } else { $null }
      firstSessionMs = if ($firstSession) { $firstSession - $startEpoch } else { $null }
    }
    treeRss         = [ordered]@{
      medianMB = $median
      minMB    = if ($sorted.Count -gt 0) { $sorted[0] } else { 0 }
      maxMB    = if ($sorted.Count -gt 0) { $sorted[$sorted.Count - 1] } else { 0 }
    }
    processes       = $rows.Count
    handles         = (($rows | Measure-Object -Property Handles -Sum).Sum)
    threads         = (($rows | Measure-Object -Property Threads -Sum).Sum)
    tabs            = $Tabs
    tabStates       = $tabStates
    driveError      = $driveError
    readyTimedOut   = $readyTimedOut
    breakdown       = $breakdown
  }

  $json = $result | ConvertTo-Json -Depth 6
  if ($Out) {
    $outPath = if ([System.IO.Path]::IsPathRooted($Out)) { $Out } else { Join-Path (Get-Location).Path $Out }
    $outDir = Split-Path $outPath -Parent
    if ($outDir) { New-Item -ItemType Directory -Force $outDir | Out-Null }
    [System.IO.File]::WriteAllText($outPath, $json, (New-Object System.Text.UTF8Encoding($false)))
  }
  $json
  [Console]::Error.WriteLine(("{0}: cold={1}ms rss_median={2}MB processes={3} ready_timeout={4}" -f $Label, $coldMs, $median, $rows.Count, $readyTimedOut))

  if ($readyTimedOut) { exit 3 }
  exit 0
} finally {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $prevWebViewArgs
  $env:ROMP_MAX_SIDECARS = $prevMaxSidecars
  if (-not $KeepRunning -and $launchedPid) {
    # The measured tree belongs to this run; leaving it alive would hold the single-instance
    # lock for the next run and leak sidecars.
    & taskkill /PID $launchedPid /T /F 2>&1 | Out-Null
  }
}
