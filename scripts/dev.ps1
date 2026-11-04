# Start `tauri dev` with the Visual Studio x64 environment and the sidecar path.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1
$ErrorActionPreference = "Stop"
$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "Visual Studio Build Tools with VC.Tools not found" }
$vc = Get-ChildItem (Join-Path $vs "VC\Tools\MSVC") -Directory | Sort-Object Name -Descending | Select-Object -First 1
$sdkRoot = "C:\Program Files (x86)\Windows Kits\10"
$sdk = Get-ChildItem (Join-Path $sdkRoot "Lib") -Directory | Sort-Object Name -Descending | Select-Object -First 1
$env:PATH = @(
  (Join-Path $vc.FullName "bin\Hostx64\x64"),
  (Join-Path $sdkRoot "bin\$($sdk.Name)\x64"),
  "$env:USERPROFILE\.cargo\bin",
  $env:PATH
) -join ";"
$env:LIB = @(
  (Join-Path $vc.FullName "lib\x64"),
  (Join-Path $sdkRoot "Lib\$($sdk.Name)\um\x64"),
  (Join-Path $sdkRoot "Lib\$($sdk.Name)\ucrt\x64")
) -join ";"
$env:INCLUDE = @(
  (Join-Path $vc.FullName "include"),
  (Join-Path $sdkRoot "Include\$($sdk.Name)\um"),
  (Join-Path $sdkRoot "Include\$($sdk.Name)\shared"),
  (Join-Path $sdkRoot "Include\$($sdk.Name)\ucrt"),
  (Join-Path $sdkRoot "Include\$($sdk.Name)\winrt")
) -join ";"
if (-not $env:OMP_SIDECAR) {
  # Repo-local sidecar first; the absolute fallback is another checkout's location.
  $repoSidecar = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..")).Path "resources\omp.exe"
  $env:OMP_SIDECAR = if (Test-Path $repoSidecar) { $repoSidecar } else { "D:\ompgui\resources\omp.exe" }
}
if (-not $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS) {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
}
$running = Get-Process romp-desktop -ErrorAction SilentlyContinue
if ($running) {
  throw "romp-desktop is already running (pid $($running.Id -join ',')). Stop it before tauri dev."
}
$log = Join-Path $PSScriptRoot "..\.probe\dev.log"
New-Item -ItemType Directory -Force (Split-Path $log) | Out-Null
Start-Transcript -Path $log -Append | Out-Null
try {
  bunx tauri dev @args
} finally {
  Stop-Transcript | Out-Null
}
