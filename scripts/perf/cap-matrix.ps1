param(
  [int]$Runs = 1,
  [string]$Exe = (Join-Path $env:LOCALAPPDATA "Romp\romp-desktop.exe"),
  [string]$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
)
$ErrorActionPreference = "Continue"
$utf8 = New-Object System.Text.UTF8Encoding($false)
$root = Join-Path $Repo ".perf\cap"
$measure = Join-Path $Repo "scripts\perf\measure.ps1"
$cdp = Join-Path $Repo "scripts\cdp-eval.mjs"
Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $root | Out-Null
$sessDir = Join-Path $env:USERPROFILE ".omp\agent\sessions\--D--rust-omp--"
$sources = @(Get-ChildItem $sessDir -Filter *.jsonl | Where-Object { $_.Name -notlike "probe-*" } | Sort-Object Length -Descending | Select-Object -First 3)
$ids = @()
foreach ($source in $sources) {
  $line = (Get-Content $source.FullName -TotalCount 2)[1]
  $ids += ($line | ConvertFrom-Json).id
}
if (($ids | Sort-Object -Unique).Count -ne 3) { throw "sources must have 3 distinct session ids, got: $($ids -join ',')" }

function Sample-Tree([int]$appPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine
  $tree = @($appPid); $added = $true
  while ($added) {
    $added = $false
    foreach ($c in $all) {
      if (($tree -contains $c.ParentProcessId) -and -not ($tree -contains $c.ProcessId)) { $tree += $c.ProcessId; $added = $true }
    }
  }
  $side = @(); $priv = 0.0; $rss = 0.0
  foreach ($id in $tree) {
    $ci = $all | Where-Object { $_.ProcessId -eq $id }
    $gp = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($gp) {
      $p = $gp.PrivateMemorySize64 / 1MB
      $priv += $p
      $rss += ($gp.WorkingSet64 / 1MB)
      if ($ci.CommandLine -like "*rpc-ui*") { $side += [math]::Round($p, 1) }
    }
  }
  $max = if ($side.Count -gt 0) { ($side | Sort-Object -Descending | Select-Object -First 1) } else { 0 }
  [pscustomobject]@{ sidecars = $side.Count; side_priv_max = $max; total_priv = [math]::Round($priv, 1); total_rss = [math]::Round($rss, 1) }
}

$rows = @()
for ($run = 1; $run -le $Runs; $run++) {
  foreach ($cap in 3, 4, 6) {
    $d = Join-Path $root ("run$run-cap$cap")
    $agentRoot = Join-Path $d "agent"
    $probeSess = Join-Path $agentRoot "sessions\--D--rust-omp--"
    New-Item -ItemType Directory -Force $probeSess | Out-Null
    $agentHome = Join-Path $env:USERPROFILE ".omp\agent"
    foreach ($name in @("models.yml", "config.yml", "models.db", "models.db-shm", "models.db-wal", "agent.db", "agent.db-shm", "agent.db-wal")) {
      $from = Join-Path $agentHome $name
      if (Test-Path $from) { Copy-Item $from (Join-Path $agentRoot $name) -Force -ErrorAction SilentlyContinue }
    }
    $env:OMP_DATA_DIR = $d
    $env:PI_CODING_AGENT_DIR = $agentRoot
    $env:ROMP_MAX_SIDECARS = "$cap"
    $copies = @()
    foreach ($source in $sources) {
      $c = Join-Path $probeSess ("probe-" + [guid]::NewGuid().ToString("N") + ".jsonl")
      Copy-Item $source.FullName $c
      $copies += $c
    }
    $null = & powershell -NoProfile -ExecutionPolicy Bypass -File $measure -Exe $Exe -Label "cap$cap" -Tabs 1 -NoDrive -NoReady -KeepRunning -WorkingDirectory $Repo 2> (Join-Path $d "measure.err")

    $sessJson = ConvertTo-Json -InputObject $copies -Compress
    $code = @"
await (async () => {
  const omp = window.omp;
  const sessions = $sessJson;
  const out = { activations: [], final: [], errors: [] };
  const first = await omp.tabs.list();
  const cwd = first.find(t => t.active)?.cwd ?? first[0]?.cwd;
  for (const sessionPath of sessions) {
    try { await omp.tabs.spawn({ cwd, sessionPath }); } catch (e) { out.errors.push(String(e)); }
  }
  let tabs = await omp.tabs.list();
  for (const t of tabs) {
    try { await omp.tabs.setActive(t.tabId); } catch (e) { out.errors.push(String(e)); }
    await new Promise(r => setTimeout(r, 800));
  }
  await new Promise(r => setTimeout(r, 12000));
  const a = await omp.tabs.list();
  await new Promise(r => setTimeout(r, 2000));
  const b = await omp.tabs.list();
  const key = list => list.map(x => x.tabId + ':' + x.status).join(',');
  out.stable = key(a) === key(b);
  out.final = b.map(t => ({ tabId: t.tabId, status: t.status }));
  return out;
})()
"@
    $exprFile = Join-Path $d "expr.js"
    [System.IO.File]::WriteAllText($exprFile, $code, $utf8)
    $driveErr = Join-Path $d "drive.err"
    $respRaw = & node $cdp "@$exprFile" 2> $driveErr | Out-String
    [System.IO.File]::WriteAllText((Join-Path $d "drive.json"), $respRaw, $utf8)
    $resp = $null
    try { $resp = $respRaw | ConvertFrom-Json } catch {}
    $dv = if ($resp -and $resp.result) { $resp.result.value } else { $null }
    $exception = if ($resp -and $resp.exceptionDetails) { $resp.exceptionDetails.exception.description } else { "" }
    $errRaw = if (Test-Path $driveErr) { Get-Content $driveErr -Raw } else { "" }
    $driveError = if ($errRaw) { $errRaw.Trim() } else { "" }

    $log = Join-Path $d "runtime.log"
    $spawns = 0; $exits = 0; $releases = 0; $falseStarts = 0
    if (Test-Path $log) {
      $spawns = @(Select-String -Path $log -Pattern "spawning sidecar").Count
      $exits = @(Select-String -Path $log -Pattern "sidecar exited").Count
      $releases = @(Select-String -Path $log -Pattern "released idle sidecar").Count
      $falseStarts = @(Select-String -Path $log -Pattern "already_started=false").Count
    }
    $app = Get-Process romp-desktop -ErrorAction SilentlyContinue | Select-Object -First 1
    $row = [pscustomobject]@{
      run = $run
      cap = $cap
      ok = [bool]$dv
      stable = if ($dv) { $dv.stable } else { $false }
      clean = if ($dv) { [bool]($dv.stable -and $spawns -le 8 -and $exits -le 4) } else { $false }
      sidecars = 0
      side_priv_max = 0
      total_priv = 0
      total_rss = 0
      ready_ms = if ($dv) { ($dv.activations | ForEach-Object { "$($_.tabId)=$($_.readyMs)" }) -join " " } else { "" }
      final = if ($dv) { ($dv.final | ForEach-Object { "$($_.tabId):$($_.status)" }) -join " " } else { "" }
      spawns = $spawns
      exits = $exits
      releases = $releases
      false_starts = $falseStarts
      errors = if ($dv) { (@($dv.errors) -join " | ") } elseif ($exception) { $exception } else { $driveError }
    }
    if ($app) {
      $s = Sample-Tree $app.Id
      $row.sidecars = $s.sidecars
      $row.side_priv_max = $s.side_priv_max
      $row.total_priv = $s.total_priv
      $row.total_rss = $s.total_rss
      & taskkill /PID $app.Id /T /F 2>&1 | Out-Null
    }
    Start-Sleep -Seconds 2
    Remove-Item $agentRoot -Recurse -Force -ErrorAction SilentlyContinue
    $rows += $row
  }
}
$rows | Format-Table -AutoSize | Out-String -Width 280
$rows | ConvertTo-Json -Depth 3 | Set-Content "$root\summary.json" -Encoding utf8
