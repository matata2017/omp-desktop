# Completes the Visual Studio 2022 Build Tools install with the C++ workload and Windows SDK.
# Downloads the bootstrapper if missing, then runs it elevated (UAC prompt).
$bootstrapper = Join-Path $env:TEMP "vs_BuildTools.exe"
if (-not (Test-Path $bootstrapper)) {
  Invoke-WebRequest -UseBasicParsing -Uri "https://aka.ms/vs/17/release/vs_BuildTools.exe" -OutFile $bootstrapper
}
$arguments = @(
  "--quiet", "--wait", "--norestart",
  "--installPath", "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools",
  "--add", "Microsoft.VisualStudio.Workload.VCTools",
  "--includeRecommended"
)
$process = Start-Process -FilePath $bootstrapper -ArgumentList $arguments -Verb RunAs -PassThru -Wait
Write-Output ("installer-exit=" + $process.ExitCode)
