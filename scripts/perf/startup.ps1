param(
  [int]$Runs = 10,
  [string]$Exe = (Join-Path $env:LOCALAPPDATA "Romp\romp-desktop.exe"),
  [string]$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
)
$ErrorActionPreference = "Continue"
$utf8 = New-Object System.Text.UTF8Encoding($false)
$root = Join-Path $Repo ".perf\startup"
$measure = Join-Path $Repo "scripts\perf\measure.ps1"
$cdp = Join-Path $Repo "scripts\cdp-eval.mjs"
Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $root | Out-Null
$agentHome = Join-Path $env:USERPROFILE ".omp\agent"

function P95($values) {
  $s = @($values | Sort-Object)
  if ($s.Count -eq 0) { return $null }
  $index = [math]::Min($s.Count - 1, [math]::Ceiling(0.95 * $s.Count) - 1)
  return $s[$index]
}

$rows = @()
for ($run = 1; $run -le $Runs; $run++) {
  $d = Join-Path $root ("run$run")
  $agentRoot = Join-Path $d "agent"
  New-Item -ItemType Directory -Force $agentRoot | Out-Null
  foreach ($name in @("models.yml", "config.yml", "models.db", "models.db-shm", "models.db-wal", "agent.db", "agent.db-shm", "agent.db-wal")) {
    $from = Join-Path $agentHome $name
    if (Test-Path $from) { Copy-Item $from (Join-Path $agentRoot $name) -Force -ErrorAction SilentlyContinue }
  }
  $env:OMP_DATA_DIR = $d
  $env:PI_CODING_AGENT_DIR = $agentRoot
  Remove-Item Env:ROMP_MAX_SIDECARS -ErrorAction SilentlyContinue
  $json = & powershell -NoProfile -ExecutionPolicy Bypass -File $measure -Exe $Exe -Label "startup$run" -Tabs 1 -NoDrive -KeepRunning -WorkingDirectory $Repo -Out (Join-Path $d "measure.json") 2> (Join-Path $d "measure.err") | ConvertFrom-Json
  $marksCode = @"
await (async () => ({
  timeOrigin: performance.timeOrigin,
  marks: performance.getEntriesByType('mark').map(entry => ({ name: entry.name, start: entry.startTime })),
}))()
"@
  $marksFile = Join-Path $d "marks.js"
  [System.IO.File]::WriteAllText($marksFile, $marksCode, $utf8)
  $marksRaw = & node $cdp "@$marksFile" 2> (Join-Path $d "marks.err") | Out-String
  $marks = $null
  try { $marks = ($marksRaw | ConvertFrom-Json).result.value } catch {}
  $startEpoch = [DateTimeOffset]::Parse($json.startedAt).ToUnixTimeMilliseconds()
  $firstPaint = $null; $firstSession = $null
  if ($marks) {
    foreach ($mark in $marks.marks) {
      $epoch = [math]::Round($marks.timeOrigin + $mark.start)
      if ($mark.name -eq "boot:first-paint" -and $null -eq $firstPaint) { $firstPaint = $epoch }
      if ($mark.name -eq "chat:first-session-rendered" -and $null -eq $firstSession) { $firstSession = $epoch }
    }
  }
  $row = [pscustomobject]@{
    run = $run
    windowMs = $json.coldStartMs
    webviewMs = if ($firstPaint) { $firstPaint - $startEpoch } else { $null }
    sidecarReadyMs = $json.milestones.sidecarReadyMs
    firstSessionMs = if ($firstSession) { $firstSession - $startEpoch } else { $null }
    privateMB = $json.treePrivate.medianMB
    rssMB = $json.treeRss.medianMB
  }
  if ($json.launchedPid) { & taskkill /PID $json.launchedPid /T /F 2>&1 | Out-Null }
  Start-Sleep -Seconds 1
  $rows += $row
}
$rows | Format-Table -AutoSize | Out-String -Width 140
foreach ($field in @("windowMs", "webviewMs", "sidecarReadyMs", "firstSessionMs")) {
  $values = @($rows | Where-Object { $null -ne $_.$field } | ForEach-Object { [double]$_.$field })
  if ($values.Count -gt 0) {
    $sorted = @($values | Sort-Object)
    $median = $sorted[[int][math]::Floor($sorted.Count / 2)]
    "$field median=$([math]::Round($median))ms p95=$(P95 $values)ms n=$($values.Count)"
  } else {
    "$field no-samples"
  }
}
$rows | ConvertTo-Json -Depth 3 | Set-Content "$root\summary.json" -Encoding utf8
