# One-command upgrade of the bundled agent sidecar (resources/omp.exe).
#
# The fork's own `omp update` cannot be used for Romp: its feed serves the STOCK
# agent, which answers "Unknown command" to get_settings_schema, terminal_list and
# get_mcp_servers (67 RPC commands vs 109 on the fork build) and would break the
# Settings, Terminal and MCP panels. The fork agent only ships inside the GUI
# installers, so this script takes it from an installer payload — an existing file
# (-Installer) or a fresh download of omp-<version>-portable.exe — runs the probe
# gate, then copies it to the three places Romp starts it from.
param(
  [string]$Installer = "",
  [string]$Version = "",
  [string]$Repo = "",
  [switch]$SkipGate,
  [switch]$SkipSwap,
  [switch]$CheckMcp,
  [string[]]$RequireMcp = @()
)
$ErrorActionPreference = "Stop"
if (-not $Repo) { $Repo = Split-Path -Parent $PSScriptRoot }
$sevenZip = "C:/7zip/7z.exe"
if (-not (Test-Path $sevenZip)) { $sevenZip = "7z" }
# 7z reports progress/errors on stderr; under $ErrorActionPreference = "Stop" that
# becomes a terminating NativeCommandError, so every 7z call runs with it relaxed.
function Invoke-SevenZip([string[]]$arguments) {
  $previous = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $null = & $sevenZip @arguments 2>&1
    return $LASTEXITCODE
  } finally { $ErrorActionPreference = $previous }
}
function Test-Installer([string]$path) { return ((Invoke-SevenZip @("t", $path)) -eq 0) }
$work = Join-Path $Repo (".resources\dl\sidecar-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Force $work | Out-Null
$dl = Join-Path $Repo ".resources\dl"
New-Item -ItemType Directory -Force $dl | Out-Null

function LatestGuiVersion {
  $tags = & git ls-remote --tags git@github.com:nornzach/oh-my-pi-gui.git "refs/tags/v0.9.*" 2>$null
  $versions = @()
  foreach ($line in $tags) {
    if ($line -match "refs/tags/v([0-9]+\.[0-9]+\.[0-9]+)$") { $versions += [version]$Matches[1] }
  }
  if ($versions.Count -eq 0) { throw "cannot resolve the latest GUI tag over git@github.com" }
  return ($versions | Sort-Object -Descending | Select-Object -First 1).ToString()
}

try {
  if (-not $Installer) {
    if (-not $Version) { $Version = LatestGuiVersion }
    $asset = "omp-$Version-portable.exe"

    $final = Join-Path $dl $asset
    if (Test-Path $final) {
      if (-not (Test-Installer $final)) {
        throw "$final is an incomplete or corrupt download (another download may still be writing it); delete it and rerun"
      }
    } else {
      $part = $final + ".part"
      $proxy = $env:HTTPS_PROXY
      $curlArgs = @("-L", "-C", "-", "--ssl-no-revoke", "-o", $part,
        "https://github.com/nornzach/oh-my-pi-gui/releases/download/v$Version/$asset")
      if ($proxy) { $curlArgs = @("-x", $proxy) + $curlArgs }
      "DOWNLOAD v$Version ($asset)"
      & curl @curlArgs
      if ($LASTEXITCODE -ne 0) { throw "download failed (exit $LASTEXITCODE); rerun to resume $part" }
      if (-not (Test-Installer $part)) { throw "download incomplete: $part ($((Get-Item $part).Length) B)" }
      Move-Item $part $final -Force
    }
    $Installer = $final
  }
  if (-not (Test-Path $Installer)) { throw "installer not found: $Installer" }

  "EXTRACT $Installer"
  $payload = Join-Path $work "payload"
  New-Item -ItemType Directory -Force $payload | Out-Null
  $exit = Invoke-SevenZip @("x", "-y", ("-o" + $payload), $Installer, '$PLUGINSDIR/app-64.7z')
  if ($exit -ne 0) { throw "7z could not open $Installer (exit $exit); the download may be incomplete" }
  $appArchive = Join-Path $payload '$PLUGINSDIR\app-64.7z'
  if (-not (Test-Path -LiteralPath $appArchive)) { throw "no `$PLUGINSDIR/app-64.7z in $Installer" }
  $app = Join-Path $work "app"
  New-Item -ItemType Directory -Force $app | Out-Null
  $null = Invoke-SevenZip @("x", "-y", ("-o" + $app), $appArchive)

  # resources/omp.exe is the agent; <app>/omp.exe is the Electron GUI binary.
  $sidecar = Join-Path $app "resources\omp.exe"
  if (-not (Test-Path $sidecar)) { throw "no resources/omp.exe inside app-64.7z" }
  $hash = (Get-FileHash -Algorithm SHA256 -Path $sidecar).Hash
  $reported = (& $sidecar --version 2>&1 | Select-Object -First 1)
  "SIDECAR $reported " + (Get-Item $sidecar).Length + " B sha256 " + $hash.Substring(0, 16)

  if (-not $SkipGate) {
    "GATE"
    # The fork proof is settings + terminals; MCP is orthogonal, so pass -CheckMcp
    # (and -RequireMcp <name>) only when you want it gated too.
    $gateArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass",
      "-File", (Join-Path $PSScriptRoot "perf\sidecar-check.ps1"), "-Exe", $sidecar)
    if ($CheckMcp) { $gateArgs += "-CheckMcp" }
    if ($RequireMcp.Count -gt 0) { $gateArgs += @("-RequireMcp") + $RequireMcp }
    & powershell @gateArgs
    if ($LASTEXITCODE -ne 0) { throw "probe gate failed; sidecar not swapped (kept in $work)" }
  }

  if (-not $SkipSwap) {
    if (Get-Process romp-desktop -ErrorAction SilentlyContinue) { throw "close romp-desktop first; it holds the sidecar" }
    $targets = @(
      (Join-Path $Repo "resources\omp.exe"),
      (Join-Path $Repo "target\release\omp.exe"),
      (Join-Path $env:LOCALAPPDATA "Romp\omp.exe")
    )
    foreach ($target in $targets) {
      if (-not (Test-Path (Split-Path -Parent $target))) { "SKIP (no dir) $target"; continue }
      if ((Test-Path $target) -and ((Get-FileHash -Algorithm SHA256 -Path $target).Hash -eq $hash)) { "SAME $target"; continue }
      if (Test-Path $target) {
        $backup = $target + ".prev"
        Copy-Item $target $backup -Force
        "BACKUP $backup"
      }
      Copy-Item $sidecar $target -Force
      "WROTE $target"
    }
    "SWAPPED $reported"
    $swapped = $true
  } else {
    "SKIPSWAP kept at $sidecar"
  }
} finally {
  if ($swapped -and (Test-Path $work)) { Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue }
}
