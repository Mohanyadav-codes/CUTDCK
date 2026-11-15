# Screenshot of the primary screen (or a window by title) for visual checks.
param([string]$Out = "C:\dev\cutdeck-shots\shot.png", [string]$Title = "")
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct RECT { public int L, T, R, B; }
}
"@
[W]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$x = $b.X; $y = $b.Y; $w = $b.Width; $h = $b.Height
if ($Title) {
  $hw = [W]::FindWindow($null, $Title)
  if ($hw -ne [IntPtr]::Zero) {
    [W]::SetForegroundWindow($hw) | Out-Null
    Start-Sleep -Milliseconds 400
    $r = New-Object W+RECT
    [W]::GetWindowRect($hw, [ref]$r) | Out-Null
    $x = $r.L; $y = $r.T; $w = $r.R - $r.L; $h = $r.B - $r.T
  }
}
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
# Downscale for quick viewing.
$scale = [Math]::Min(1.0, 1400 / $w)
$sw = [int]($w * $scale); $sh = [int]($h * $scale)
$small = New-Object System.Drawing.Bitmap $sw, $sh
$g2 = [System.Drawing.Graphics]::FromImage($small)
$g2.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g2.DrawImage($bmp, 0, 0, $sw, $sh)
$small.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"$Out ${w}x${h}"
