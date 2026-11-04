# Capture the Romp window to a PNG for smoke-test evidence.
# Usage: powershell -NoProfile -File scripts/capture-window.ps1 -Out screenshot.png
param(
  [string]$ProcessName = "romp-desktop",
  [string]$Title = "Romp",
  [string]$Out = "screenshot.png"
)

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

$proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -like "*$Title*" } |
  Select-Object -First 1
if (-not $proc) {
  $proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
}
if (-not $proc) { Write-Output "no-window"; exit 2 }

[Win32]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
[Win32]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 700

$rect = New-Object Win32+RECT
[Win32]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
if ($w -le 0 -or $h -le 0) { Write-Output "bad-rect $w x $h"; exit 3 }

$bmp = New-Object System.Drawing.Bitmap $w, $h
$gfx = [System.Drawing.Graphics]::FromImage($bmp)
$gfx.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bmp.Size)
$bmp.Save((Resolve-Path -LiteralPath (Split-Path -Parent $Out)).Path + "\" + (Split-Path -Leaf $Out), [System.Drawing.Imaging.ImageFormat]::Png)
$gfx.Dispose()
$bmp.Dispose()

Write-Output ("captured {0} {1}x{2} title='{3}' pid={4}" -f $Out, $w, $h, $proc.MainWindowTitle, $proc.Id)
