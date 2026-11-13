# Upgrade gate for resources/omp.exe: does the candidate sidecar still speak the
# fork RPC surface Romp depends on? Stock omp answers "Unknown command" to
# get_settings_schema / terminal_list, so those two successes are the fork proof;
# get_available_commands supplies the command inventory diff, and a set_todos
# round-trip proves the todo dock's statuses survive (a sidecar may answer success
# while normalizing a status away). With -CheckMcp the probe copies the user
# mcp.json into the isolated agent root and reports each server's status
# (-RequireMcp turns a named server into a gate).
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$Repo = "",
  [string]$Out = "",
  [switch]$CheckMcp,
  [string[]]$RequireMcp = @()
)
$ErrorActionPreference = "Continue"
if (-not $Repo) { $Repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot) }
# & only executes extensions on PATHEXT, so a candidate saved as omp.exe.prev is
# probed through a temp .exe copy; the process itself starts fine either way.
$probeExe = $Exe
if ([System.IO.Path]::GetExtension($Exe) -ne ".exe") {
  $probeExe = Join-Path $env:TEMP ("sidecar-probe-" + [guid]::NewGuid().ToString("N").Substring(0, 8) + ".exe")
  Copy-Item $Exe $probeExe -Force
}
$rawVersion = (& $probeExe --version 2>&1 | Select-Object -First 1)
"EXE=$Exe"
"VERSION=$rawVersion"
"BYTES=" + (Get-Item $Exe).Length
"SHA256=" + (Get-FileHash -Algorithm SHA256 -Path $Exe).Hash.Substring(0, 16)
if (-not $Out) { $Out = Join-Path $Repo ".perf\sidecar-check\$version" }
New-Item -ItemType Directory -Force $Out | Out-Null

$root = Join-Path $env:TEMP ("sidecar-check-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Force $root | Out-Null
$agentHome = Join-Path $env:USERPROFILE ".omp\agent"
foreach ($name in @("models.yml", "config.yml", "models.db", "models.db-shm", "models.db-wal", "agent.db", "agent.db-shm", "agent.db-wal")) {
  $from = Join-Path $agentHome $name
  if (Test-Path $from) { Copy-Item $from (Join-Path $root $name) -Force -ErrorAction SilentlyContinue }
}
if ($CheckMcp) {
  $mcp = Join-Path $agentHome "mcp.json"
  if (Test-Path $mcp) { Copy-Item $mcp (Join-Path $root "mcp.json") -Force } else { "MCP_SOURCE=missing" }
}

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Exe
$psi.Arguments = "--mode rpc-ui"
$psi.WorkingDirectory = $Repo
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.EnvironmentVariables["PI_CODING_AGENT_DIR"] = $root
$p = New-Object System.Diagnostics.Process
$p.StartInfo = $psi
$null = $p.Start()

function Send-Until([string]$needle, [int]$timeoutMs, [string]$body) {
  if ($body) { $p.StandardInput.WriteLine($body); $p.StandardInput.Flush() }
  $deadline = (Get-Date).AddMilliseconds($timeoutMs)
  while ((Get-Date) -lt $deadline) {
    $remaining = [int][Math]::Max(1, ($deadline - (Get-Date)).TotalMilliseconds)
    $task = $p.StandardOutput.ReadLineAsync()
    if (-not $task.Wait($remaining)) { return $null }
    if ($null -eq $task.Result) { return $null }
    if ($task.Result.Contains($needle)) { return $task.Result }
  }
  return $null
}

$ready = Send-Until '"type":"ready"' 40000 ""
"READY=$ready"
$exitCode = 0
if (-not $ready) {
  "RESULT=FAIL no ready frame within 40s"
  if (-not $p.HasExited) { & taskkill /PID $p.Id /T /F 2>&1 | Out-Null }
  exit 1
}

$checks = [ordered]@{
  settings  = @{ id = "c1"; body = '{"id":"c1","type":"get_settings_schema"}' }
  terminals = @{ id = "c2"; body = '{"id":"c2","type":"terminal_list"}' }
  commands  = @{ id = "c3"; body = '{"id":"c3","type":"get_available_commands"}' }
}
if ($CheckMcp) { $checks["mcp"] = @{ id = "c4"; body = '{"id":"c4","type":"get_mcp_servers"}' } }
# The todo dock pushes every status the renderer can produce; a sidecar that
# normalizes one away still answers success, so assert the echoed phases.
$checks["todos"] = @{ id = "ct"; body = '{"id":"ct","type":"set_todos","phases":[{"name":"gate","tasks":[{"content":"gate","status":"blocked"}]}]}' }

foreach ($name in $checks.Keys) {
  $spec = $checks[$name]
  $line = Send-Until ('"id":"' + $spec.id + '"') 60000 $spec.body
  if (-not $line) { "$name=NO_RESPONSE"; $exitCode = 1; continue }
  $obj = $null
  try { $obj = $line | ConvertFrom-Json } catch {}
  "$name success=$($obj.success) bytes=$($line.Length)"
  [System.IO.File]::WriteAllText((Join-Path $Out ($name + ".json")), $line, (New-Object System.Text.UTF8Encoding($false)))
  if (-not $obj.success) {
    $exitCode = 1
    "  error=$($obj.error)"
    continue
  }
  if ($name -eq "commands") {
    $data = $obj.data
    $list = if ($data -is [array]) { $data } else { $data.commands }
    "  command_count=$($list.Count)"
    $names = ($list | ForEach-Object { $_.name } | Sort-Object)
    [System.IO.File]::WriteAllText((Join-Path $Out "command-names.txt"), ($names -join "`n"), (New-Object System.Text.UTF8Encoding($false)))
  }
  if ($name -eq "mcp") {
    # stdio servers (krouter, tinyfish) report "connecting" right after spawn;
    # re-query until every status is terminal so the probe shows the real outcome.
    for ($try = 1; $try -le 10; $try++) {
      $pending = @($obj.data.servers | Where-Object { $_.status -eq "connecting" })
      if ($pending.Count -eq 0) { break }
      Start-Sleep -Milliseconds 1000
      $again = Send-Until '"id":"c4"' 20000 $spec.body
      if (-not $again) { break }
      $line = $again
      try { $obj = $line | ConvertFrom-Json } catch { break }
    }
    [System.IO.File]::WriteAllText((Join-Path $Out ($name + ".json")), $line, (New-Object System.Text.UTF8Encoding($false)))
    foreach ($server in $obj.data.servers) { "  mcp $($server.name) status=$($server.status) tools=$($server.toolCount)" }
    foreach ($server in $obj.data.servers) { if ($server.status -ne "connected") { "  mcp $($server.name) status=$($server.status) (not a gate)" } }
    foreach ($required in $RequireMcp) {
      $hit = $obj.data.servers | Where-Object { $_.name -eq $required -and $_.status -eq "connected" }
      if (-not $hit) { "  mcp $required REQUIRED_BUT_NOT_CONNECTED"; $exitCode = 1 }
    }
  }
  if ($name -eq "todos") {
    $echo = @($obj.data.todoPhases)[0].tasks[0].status
    "  todo_echo=$echo"
    if ($echo -ne "blocked") { "  todo status blocked NOT echoed back (got '$echo')"; $exitCode = 1 }
  }
}

if (-not $p.HasExited) { & taskkill /PID $p.Id /T /F 2>&1 | Out-Null }
Start-Sleep -Milliseconds 300
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
if ($probeExe -ne $Exe) { Remove-Item $probeExe -Force -ErrorAction SilentlyContinue }
"OUT=$Out"
"RESULT=" + $(if ($exitCode -eq 0) { "OK" } else { "FAIL" })
exit $exitCode
