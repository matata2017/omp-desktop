param(
  [int]$Batches = 10,
  [int]$PerBatch = 10,
  [string]$Exe = (Join-Path $env:LOCALAPPDATA "Romp\romp-desktop.exe"),
  [string]$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path,
  [switch]$RawClose
)
$ErrorActionPreference = "Continue"
$utf8 = New-Object System.Text.UTF8Encoding($false)
$root = Join-Path $Repo ".perf\lifecycle"
$measure = Join-Path $Repo "scripts\perf\measure.ps1"
$cdp = Join-Path $Repo "scripts\cdp-eval.mjs"
$realistic = if ($RawClose) { "false" } else { "true" }
Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $root | Out-Null
$agentHome = Join-Path $env:USERPROFILE ".omp\agent"

function Sample-Tree([int]$appPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine
  $tree = @($appPid); $added = $true
  while ($added) {
    $added = $false
    foreach ($c in $all) {
      if (($tree -contains $c.ParentProcessId) -and -not ($tree -contains $c.ProcessId)) { $tree += $c.ProcessId; $added = $true }
    }
  }
  $side = 0; $priv = 0.0; $rss = 0.0; $handles = 0; $threads = 0
  foreach ($id in $tree) {
    $ci = $all | Where-Object { $_.ProcessId -eq $id }
    $gp = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($gp) {
      $priv += $gp.PrivateMemorySize64 / 1MB
      $rss += $gp.WorkingSet64 / 1MB
      $handles += $gp.HandleCount
      $threads += $gp.Threads.Count
      if ($ci.CommandLine -like "*rpc-ui*") { $side += 1 }
    }
  }
  [pscustomobject]@{ sidecars = $side; privateMB = [math]::Round($priv, 1); rssMB = [math]::Round($rss, 1); handles = $handles; threads = $threads }
}

$d = Join-Path $root "app"
$agentRoot = Join-Path $d "agent"
New-Item -ItemType Directory -Force $agentRoot | Out-Null
foreach ($name in @("models.yml", "config.yml", "models.db", "models.db-shm", "models.db-wal", "agent.db", "agent.db-shm", "agent.db-wal")) {
  $from = Join-Path $agentHome $name
  if (Test-Path $from) { Copy-Item $from (Join-Path $agentRoot $name) -Force -ErrorAction SilentlyContinue }
}
$env:OMP_DATA_DIR = $d
$env:PI_CODING_AGENT_DIR = $agentRoot
$env:ROMP_MAX_SIDECARS = "6"
$json = & powershell -NoProfile -ExecutionPolicy Bypass -File $measure -Exe $Exe -Label lifecycle -Tabs 1 -NoDrive -KeepRunning -WorkingDirectory $Repo -Out (Join-Path $d "measure.json") 2> (Join-Path $d "measure.err") | ConvertFrom-Json
$app = Get-Process -Id $json.launchedPid -ErrorAction SilentlyContinue
if (-not $app) { throw "app not running" }

$rows = @()
$baseline = Sample-Tree $app.Id
$rows += [pscustomobject]@{ cycles = 0; sidecars = $baseline.sidecars; privateMB = $baseline.privateMB; rssMB = $baseline.rssMB; handles = $baseline.handles; threads = $baseline.threads }

$code = @"
await (async () => {
  const omp = window.omp;
  const first = await omp.tabs.list();
  const cwd = first[0]?.cwd ?? 'D:/rust-omp';
  let done = 0;
  for (let i = 0; i < $PerBatch; i += 1) {
    const tab = await omp.tabs.spawn({ cwd });
    if (tab && tab.tabId) { if ($realistic) { await omp.tabs.setActive(first[0].tabId); } await omp.tabs.close(tab.tabId); done += 1; }
    await new Promise(r => setTimeout(r, 100));
  }
  return done;
})()
"@
$loopFile = Join-Path $d "loop.js"
[System.IO.File]::WriteAllText($loopFile, $code, $utf8)

for ($batch = 1; $batch -le $Batches; $batch++) {
  $raw = & node $cdp "@$loopFile" 2> (Join-Path $d "loop.err") | Out-String
  $done = 0
  try { $done = ($raw | ConvertFrom-Json).result.value } catch {}
  Start-Sleep -Seconds 1
  $s = Sample-Tree $app.Id
  $rows += [pscustomobject]@{ cycles = $batch * $PerBatch; sidecars = $s.sidecars; privateMB = $s.privateMB; rssMB = $s.rssMB; handles = $s.handles; threads = $s.threads }
  "batch $batch done=$done private=$($s.privateMB) rss=$($s.rssMB) sidecars=$($s.sidecars)"
}
& taskkill /PID $app.Id /T /F 2>&1 | Out-Null
$rows | Format-Table -AutoSize | Out-String -Width 120
$first = $rows[0]; $last = $rows[$rows.Count - 1]
"cycles=$($last.cycles) dPrivate=$([math]::Round($last.privateMB - $first.privateMB, 1))MB dRss=$([math]::Round($last.rssMB - $first.rssMB, 1))MB dHandles=$($last.handles - $first.handles) dThreads=$($last.threads - $first.threads)"
$rows | ConvertTo-Json -Depth 3 | Set-Content "$root\summary.json" -Encoding utf8
