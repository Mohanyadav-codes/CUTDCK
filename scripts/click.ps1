# Click at screen coordinates (physical pixels), for UI checks.
param([int]$X, [int]$Y, [switch]$Double)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class M {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[M]::SetProcessDPIAware() | Out-Null
[M]::SetCursorPos($X, $Y) | Out-Null
Start-Sleep -Milliseconds 150
$n = if ($Double) { 2 } else { 1 }
for ($i = 0; $i -lt $n; $i++) { [M]::mouse_event(2, 0, 0, 0, [IntPtr]::Zero); [M]::mouse_event(4, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 80 }
