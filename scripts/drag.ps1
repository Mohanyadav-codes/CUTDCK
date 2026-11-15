param([int]$X1,[int]$Y1,[int]$X2,[int]$Y2)
Add-Type @"
using System; using System.Runtime.InteropServices;
public class D { [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,IntPtr e);
[DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
"@
[D]::SetProcessDPIAware() | Out-Null
[D]::SetCursorPos($X1,$Y1) | Out-Null; Start-Sleep -Milliseconds 400
[D]::mouse_event(2,0,0,0,[IntPtr]::Zero); Start-Sleep -Milliseconds 120
for($i=1;$i -le 40;$i++){ $x=[int]($X1+($X2-$X1)*$i/40); $y=[int]($Y1+($Y2-$Y1)*$i/40); [D]::SetCursorPos($x,$y) | Out-Null; Start-Sleep -Milliseconds 15 }
Start-Sleep -Milliseconds 200
[D]::mouse_event(4,0,0,0,[IntPtr]::Zero)
