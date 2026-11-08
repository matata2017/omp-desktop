param(
  [int]$Runs = 3,
  [string]$Exe = (Join-Path $env:LOCALAPPDATA "Romp\romp-desktop.exe"),
  [string]$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
)
$ErrorActionPreference = "Continue"
$utf8 = New-Object System.Text.UTF8Encoding($false)
$root = Join-Path $Repo ".perf\gpu-ab"
Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $root | Out-Null
$shots = Join-Path $root "shots"
New-Item -ItemType Directory -Force $shots | Out-Null
$agentHome = Join-Path $env:USERPROFILE ".omp\agent"

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class GpuAbWin32 {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

function Sample-Tree([int]$appPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine
  $tree = @($appPid); $added = $true
  while ($added) {
    $added = $false
    foreach ($c in $all) {
      if (($tree -contains $c.ParentProcessId) -and -not ($tree -contains $c.ProcessId)) { $tree += $c.ProcessId; $added = $true }
    }
  }
  $priv = 0.0; $rss = 0.0; $side = 0
  $wvPriv = 0.0; $wvRss = 0.0; $wvCount = 0
  $gpuPriv = 0.0; $gpuRss = 0.0; $gpuPresent = $false
  foreach ($id in $tree) {
    $ci = $all | Where-Object { $_.ProcessId -eq $id }
    $gp = Get-Process -Id $id -ErrorAction SilentlyContinue
    if (-not $gp) { continue }
    $p = $gp.PrivateMemorySize64 / 1MB
    $r = $gp.WorkingSet64 / 1MB
    $priv += $p; $rss += $r
    if ($ci.CommandLine -like "*rpc-ui*") { $side += 1 }
    if ($ci.Name -like "msedgewebview2*") {
      $wvPriv += $p; $wvRss += $r; $wvCount += 1
      if ($ci.CommandLine -like "*--type=gpu-process*") { $gpuPriv += $p; $gpuRss += $r; $gpuPresent = $true }
    }
  }
  [pscustomobject]@{
    totalPrivate = [math]::Round($priv, 1); totalRss = [math]::Round($rss, 1); sidecars = $side
    webviewPrivate = [math]::Round($wvPriv, 1); webviewRss = [math]::Round($wvRss, 1); webviewProcs = $wvCount
    gpuPrivate = [math]::Round($gpuPriv, 1); gpuRss = [math]::Round($gpuRss, 1); gpuPresent = $gpuPresent
  }
}

function Capture-Window([string]$path) {
  $app = Get-Process romp-desktop -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $app -or $app.MainWindowHandle -eq 0) { return $false }
  [GpuAbWin32]::SetForegroundWindow($app.MainWindowHandle) | Out-Null
  Start-Sleep -Milliseconds 600
  $rect = New-Object GpuAbWin32+RECT
  if (-not [GpuAbWin32]::GetWindowRect($app.MainWindowHandle, [ref]$rect)) { return $false }
  $w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
  if ($w -le 0 -or $h -le 0) { return $false }
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bmp.Size)
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  return $true
}

$uiCode = @"
await (async () => ({
  title: document.title,
  nodes: document.querySelectorAll('*').length,
  rootChildren: document.querySelector('#root')?.children.length ?? 0,
  text: (document.body.innerText || '').slice(0, 120),
  bridge: typeof window.omp,
}))()
"@

$rows = @()
foreach ($arm in @(@{ name = "baseline"; args = "" }, @{ name = "disable-gpu"; args = "--disable-gpu" })) {
  for ($run = 1; $run -le $Runs; $run++) {
    $d = Join-Path $root ("$($arm.name)-$run")
    $agentRoot = Join-Path $d "agent"
    New-Item -ItemType Directory -Force $agentRoot | Out-Null
    foreach ($name in @("models.yml", "config.yml", "models.db", "models.db-shm", "models.db-wal", "agent.db", "agent.db-shm", "agent.db-wal")) {
      $from = Join-Path $agentHome $name
      if (Test-Path $from) { Copy-Item $from (Join-Path $agentRoot $name) -Force -ErrorAction SilentlyContinue }
    }
    $env:OMP_DATA_DIR = $d
    $env:PI_CODING_AGENT_DIR = $agentRoot
    Remove-Item Env:ROMP_MAX_SIDECARS -ErrorAction SilentlyContinue
    $psArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $Repo "scripts\perf\measure.ps1"), "-Exe", $Exe, "-Label", "$($arm.name)-$run", "-Tabs", "1", "-NoDrive", "-KeepRunning", "-WorkingDirectory", $Repo, "-Out", (Join-Path $d "measure.json"))
    if ($arm.args) { $psArgs += @("-BrowserArgs", $arm.args) }
    $json = & powershell @psArgs 2> (Join-Path $d "measure.err") | ConvertFrom-Json
    if (-not $json -or -not $json.launchedPid) { throw "measure failed for $($arm.name)-$run; see $d\measure.err" }
    Start-Sleep -Seconds 1

    $s = Sample-Tree $json.launchedPid
    $uiFile = Join-Path $d "ui.js"
    [System.IO.File]::WriteAllText($uiFile, $uiCode, $utf8)
    $uiRaw = & node (Join-Path $Repo "scripts\cdp-eval.mjs") "@$uiFile" 2> (Join-Path $d "ui.err") | Out-String
    $ui = $null
    try { $ui = ($uiRaw | ConvertFrom-Json).result.value } catch {}

    $shotPath = Join-Path $shots ("$($arm.name)-$run.png")
    $shotOk = Capture-Window $shotPath
    $row = [pscustomobject]@{
      arm = $arm.name
      run = $run
      windowMs = $json.coldStartMs
      sidecarReadyMs = $json.milestones.sidecarReadyMs
      totalPrivate = $s.totalPrivate
      totalRss = $s.totalRss
      webviewPrivate = $s.webviewPrivate
      webviewRss = $s.webviewRss
      webviewProcs = $s.webviewProcs
      gpuPrivate = $s.gpuPrivate
      gpuRss = $s.gpuRss
      gpuPresent = $s.gpuPresent
      sidecars = $s.sidecars
      rootChildren = if ($ui) { $ui.rootChildren } else { -1 }
      nodes = if ($ui) { $ui.nodes } else { -1 }
      shot = $shotOk
    }
    if ($json.launchedPid) { & taskkill /PID $json.launchedPid /T /F 2>&1 | Out-Null }
    Start-Sleep -Seconds 2
    $rows += $row
    $row | Format-Table -AutoSize | Out-String -Width 220 | Write-Host
  }
}

$rows | Format-Table -AutoSize | Out-String -Width 240
foreach ($arm in @("baseline", "disable-gpu")) {
  $subset = @($rows | Where-Object { $_.arm -eq $arm })
  foreach ($field in @("windowMs", "sidecarReadyMs", "totalPrivate", "totalRss", "webviewPrivate", "webviewRss", "gpuPrivate", "gpuRss")) {
    $values = @($subset | ForEach-Object { [double]$_.$field } | Sort-Object)
    if ($values.Count -gt 0) {
      $median = $values[[int][math]::Floor($values.Count / 2)]
      "$arm $field median=$([math]::Round($median, 1)) range=$([math]::Round($values[0], 1))..$([math]::Round($values[$values.Count - 1], 1))"
    }
  }
}
$rows | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $root "summary.json") -Encoding utf8
