# Probe the bundled omp.exe RPC surface: is there session routing, or only one
# serial active session? Evidence for the "shared sidecar" decision.
param(
  [string]$Exe = (Join-Path $env:LOCALAPPDATA "Romp\omp.exe"),
  [string]$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
)
$ErrorActionPreference = "Continue"
$root = Join-Path $Repo ".perf\rpc-caps"
New-Item -ItemType Directory -Force $root | Out-Null
$agentHome = Join-Path $env:USERPROFILE ".omp\agent"
foreach ($name in @("models.yml", "config.yml", "models.db", "models.db-shm", "models.db-wal", "agent.db", "agent.db-shm", "agent.db-wal")) {
  $from = Join-Path $agentHome $name
  if (Test-Path $from) { Copy-Item $from (Join-Path $root $name) -Force -ErrorAction SilentlyContinue }
}
$sessDir = Join-Path $root "sessions\ws1"
New-Item -ItemType Directory -Force $sessDir | Out-Null

function Read-Until($proc, [string]$needle, [int]$timeoutMs) {
  $deadline = (Get-Date).AddMilliseconds($timeoutMs)
  $skipped = 0
  while ((Get-Date) -lt $deadline) {
    $remaining = [int][Math]::Max(1, ($deadline - (Get-Date)).TotalMilliseconds)
    $task = $proc.StandardOutput.ReadLineAsync()
    if (-not $task.Wait($remaining)) { return [pscustomobject]@{ line = $null; skipped = $skipped } }
    $line = $task.Result
    if ($null -eq $line) { return [pscustomobject]@{ line = $null; skipped = $skipped } }
    if ($line.Contains($needle)) { return [pscustomobject]@{ line = $line; skipped = $skipped } }
    $skipped += 1
  }
  return [pscustomobject]@{ line = $null; skipped = $skipped }
}

function Sha256([string]$text) {
  $bytes = [Text.Encoding]::UTF8.GetBytes($text)
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try { [BitConverter]::ToString($sha.ComputeHash($bytes)) } finally { $sha.Dispose() }
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

$ready = Read-Until $p '"type":"ready"' 20000
"READY_FOUND=$([bool]$ready.line) skipped=$($ready.skipped)"
if ($ready.line) { "READY=" + ($ready.line -replace "\s+", " ").Substring(0, [Math]::Min(220, ($ready.line -replace "\s+", " ").Length)) }
if (-not $ready.line) { if (-not $p.HasExited) { & taskkill /PID $p.Id /T /F 2>&1 | Out-Null }; exit 1 }

$commands = @(
  @{ id = "c1"; body = '{"id":"c1","type":"get_state"}' },
  @{ id = "c2"; body = '{"id":"c2","type":"get_state","sessionId":"00000000-0000-0000-0000-000000000000"}' },
  @{ id = "c3"; body = '{"id":"c3","type":"get_session_lease","sessionIds":[]}' },
  @{ id = "c4"; body = '{"id":"c4","type":"get_available_commands"}' },
  @{ id = "c5"; body = ("{""id"":""c5"",""type"":""open_session"",""sessionDir"":""" + ($sessDir -replace "\\", "/") + """}" ) }
)

$responses = @{}
foreach ($cmd in $commands) {
  $p.StandardInput.WriteLine($cmd.body)
  $p.StandardInput.Flush()
  $r = Read-Until $p ('"id":"' + $cmd.id + '"') 12000
  if (-not $r.line) {
    "CMD $($cmd.id) NO_RESPONSE_OR_TIMEOUT skipped=$($r.skipped) - aborting (pending read left on stream)"
    break
  }
  $responses[$cmd.id] = $r.line
  $obj = $null
  try { $obj = $r.line | ConvertFrom-Json } catch {}
  "CMD $($cmd.id) success=$($obj.success) command=$($obj.command) bytes=$($r.line.Length)"
}

if ($responses["c1"] -and $responses["c2"]) {
  $c1 = ($responses["c1"] | ConvertFrom-Json).data | ConvertTo-Json -Compress -Depth 12
  $c2 = ($responses["c2"] | ConvertFrom-Json).data | ConvertTo-Json -Compress -Depth 12
  "C1_DATA_SHA=$(Sha256 $c1) C2_DATA_SHA=$(Sha256 $c2) DATA_IDENTICAL=$($c1 -eq $c2)"
  $stateKeys = @(($responses["c1"] | ConvertFrom-Json).data.PSObject.Properties.Name)
  "STATE_KEYS_WITH_SESSION=" + (@($stateKeys | Where-Object { $_ -match "session" }) -join ",")
}

if ($responses["c4"]) {
  $c4 = ($responses["c4"] | ConvertFrom-Json).data
  $commandsPath = Join-Path $root "commands.json"
  [System.IO.File]::WriteAllText($commandsPath, ($c4.commands | ConvertTo-Json -Depth 12), (New-Object System.Text.UTF8Encoding($false)))
  $schemaText = Get-Content $commandsPath -Raw
  "COMMAND_COUNT=$($c4.commands.Count) SCHEMA_FILE=$commandsPath"
  $routed = @($c4.commands | Where-Object { (($_.input | ConvertTo-Json -Compress -Depth 8) -match '"sessionId"') })
  "COMMANDS_WITH_SESSIONID_INPUT=$($routed.Count)"
  "SESSIONID_IN_SCHEMA_OCCURRENCES=" + ([regex]::Matches($schemaText, "sessionId").Count)
}

if (-not $p.HasExited) { & taskkill /PID $p.Id /T /F 2>&1 | Out-Null }
