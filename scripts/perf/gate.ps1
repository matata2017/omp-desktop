# Compare two measurement JSON files and fail when a budget regresses.
# Exit 0 pass, 1 regression, 2 unreadable or missing fields.
# Usage: powershell -NoProfile -File scripts/perf/gate.ps1 -Baseline a.json -Current b.json [-MaxRegressionPct 10] [-MaxRssMB 900]
param(
  [Parameter(Mandatory = $true)][string]$Baseline,
  [Parameter(Mandatory = $true)][string]$Current,
  [double]$MaxRegressionPct = 10,
  [double]$MaxRssMB = 0,
  [double]$MaxColdStartMs = 0
)

$ErrorActionPreference = "Stop"

function Read-Measurement {
  param([string]$Path)
  if (-not (Test-Path $Path)) { throw "missing file: $Path" }
  $value = Get-Content $Path -Raw | ConvertFrom-Json
  foreach ($field in @("coldStartMs", "treeRss")) {
    if ($null -eq $value.$field) { throw "missing field '$field' in $Path" }
  }
  if ($null -eq $value.treeRss.medianMB) { throw "missing field 'treeRss.medianMB' in $Path" }
  $value
}

try {
  $base = Read-Measurement $Baseline
  $cur = Read-Measurement $Current
} catch {
  Write-Output "gate error: $($_.Exception.Message)"
  exit 2
}

$failures = @()
$notes = @()

if ($cur.readyTimedOut -eq $true) {
  $failures += "readyTimedOut=true"
}

$ratio = 1 + ($MaxRegressionPct / 100)
# Cold start is noisy (275-694 ms on the same machine, OS/WebView2 cache), so a tight
# relative comparison flags normal variance. When an absolute budget is supplied, use it
# for cold start instead; RSS is stable and keeps both relative and absolute checks.
if ($MaxColdStartMs -gt 0) {
  if ($cur.coldStartMs -gt $MaxColdStartMs) {
    $failures += "coldStartMs $($cur.coldStartMs) > budget $MaxColdStartMs"
  } else {
    $notes += "coldStartMs $($cur.coldStartMs) <= budget $MaxColdStartMs"
  }
} else {
  $coldLimit = [math]::Round($base.coldStartMs * $ratio)
  if ($cur.coldStartMs -gt $coldLimit) {
    $failures += "coldStartMs $($cur.coldStartMs) > $coldLimit (baseline $($base.coldStartMs), +$MaxRegressionPct%)"
  } else {
    $notes += "coldStartMs $($cur.coldStartMs) <= $coldLimit"
  }
}

$rssLimit = [math]::Round($base.treeRss.medianMB * $ratio, 1)
if ($cur.treeRss.medianMB -gt $rssLimit) {
  $failures += "treeRss.medianMB $($cur.treeRss.medianMB) > $rssLimit (baseline $($base.treeRss.medianMB), +$MaxRegressionPct%)"
} else {
  $notes += "treeRss.medianMB $($cur.treeRss.medianMB) <= $rssLimit"
}

if ($MaxRssMB -gt 0 -and $cur.treeRss.medianMB -gt $MaxRssMB) {
  $failures += "treeRss.medianMB $($cur.treeRss.medianMB) > budget $MaxRssMB"
}


foreach ($note in $notes) { Write-Output "ok: $note" }
if ($failures.Count -gt 0) {
  foreach ($failure in $failures) { Write-Output "fail: $failure" }
  exit 1
}
Write-Output "gate passed: $($cur.label)"
exit 0
