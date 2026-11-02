param([string]$ExecutablePath,[switch]$CloseAfter)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$appVersion=(Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
$executable = if($ExecutablePath){[IO.Path]::GetFullPath($ExecutablePath)}else{Join-Path $projectRoot ('artifacts\leafshield-'+$appVersion+'.exe')}
if (Get-Process -Name leafshield,'leafshield-*' -ErrorAction SilentlyContinue) { throw 'Close the existing app before this interactive desktop test.' }
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class LeafDockNative {
  [StructLayout(LayoutKind.Sequential)] public struct RECT {public int Left,Top,Right,Bottom;}
  [StructLayout(LayoutKind.Sequential)] public struct POINT {public int X,Y;}
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO {public int cbSize;public RECT Monitor,Work;public uint Flags;}
  private delegate bool EnumProc(IntPtr window,IntPtr parameter);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback,IntPtr parameter);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window,out uint processId);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window,StringBuilder text,int count);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out RECT rect);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window,int x,int y,int width,int height,bool repaint);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT point);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr window,uint flags);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern bool GetMonitorInfo(IntPtr monitor,ref MONITORINFO info);
  public static IntPtr Find(uint processId,string titlePart) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((window,unused)=>{uint current;GetWindowThreadProcessId(window,out current);if(current==processId){var title=new StringBuilder(512);GetWindowText(window,title,title.Capacity);if(title.ToString().Contains(titlePart)){found=window;return false;}}return true;},IntPtr.Zero);
    return found;
  }
}
'@
$previousContext = [LeafDockNative]::SetThreadDpiAwarenessContext([IntPtr]::new(-4))
$originalPointer = New-Object LeafDockNative+POINT
[void][LeafDockNative]::GetCursorPos([ref]$originalPointer)
$widget = [IntPtr]::Zero
$originalFrame = $null
function Read-Rect([IntPtr]$window) { $rect = New-Object LeafDockNative+RECT; if (-not [LeafDockNative]::GetWindowRect($window,[ref]$rect)) { throw 'Could not inspect native window bounds.' }; return $rect }
function Rect-Data($rect) { [ordered]@{x=$rect.Left;y=$rect.Top;width=$rect.Right-$rect.Left;height=$rect.Bottom-$rect.Top} }
try {
  $process = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
  for ($attempt=0;$attempt -lt 80;$attempt++) { $widget=[LeafDockNative]::Find($process.Id,'悬浮计时');if($widget -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 150 }
  if ($widget -eq [IntPtr]::Zero) { throw 'The native floating timer did not appear.' }
  $originalFrame=Read-Rect $widget
  $main=[LeafDockNative]::Find($process.Id,'LeafShield')
  $mainFrame=Read-Rect $main
  $scale=[LeafDockNative]::GetDpiForWindow($widget)/96.0
  $info=New-Object LeafDockNative+MONITORINFO
  $info.cbSize=[Runtime.InteropServices.Marshal]::SizeOf($info)
  if(-not [LeafDockNative]::GetMonitorInfo([LeafDockNative]::MonitorFromWindow($widget,2),[ref]$info)){throw 'Could not inspect the monitor work area.'}
  $work=$info.Work;$width=$originalFrame.Right-$originalFrame.Left;$height=$originalFrame.Bottom-$originalFrame.Top
  $outsideX=$work.Left+[int](($work.Right-$work.Left)/2);$outsideY=$work.Top+[int](($work.Bottom-$work.Top)/2)
  $results=@()
  foreach($edge in @('left','right','top','bottom','left-outside','right-outside','top-outside','bottom-outside')) {
    [void][LeafDockNative]::SetCursorPos($outsideX,$outsideY)
    [void][LeafDockNative]::MoveWindow($widget,$originalFrame.Left,$originalFrame.Top,$width,$height,$true)
    Start-Sleep -Milliseconds 400
    $x=$work.Left+[int](($work.Right-$work.Left)/3);$y=$work.Top+[int](180*$scale)
    if($edge -eq 'left'){$x=$work.Left+[int](3*$scale)}
    if($edge -eq 'right'){$x=$work.Right-$width-[int](3*$scale)}
    if($edge -eq 'top'){$y=$work.Top+[int](3*$scale)}
    if($edge -eq 'bottom'){$y=$work.Bottom-$height-[int](3*$scale)}
    if($edge -eq 'left-outside'){$x=$work.Left-[int](74*$scale)}
    if($edge -eq 'right-outside'){$x=$work.Right-$width+[int](74*$scale)}
    if($edge -eq 'top-outside'){$y=$work.Top-[int](20*$scale)}
    if($edge -eq 'bottom-outside'){$y=$work.Bottom-$height+[int](20*$scale)}
    [void][LeafDockNative]::MoveWindow($widget,$x,$y,$width,$height,$true)
    $collapsed=$null
    for($attempt=0;$attempt -lt 30;$attempt++) {
      Start-Sleep -Milliseconds 120
      $rect=Read-Rect $widget
      $thin=if($edge -like 'left*' -or $edge -like 'right*'){$rect.Right-$rect.Left}else{$rect.Bottom-$rect.Top}
      if($thin -le [Math]::Ceiling(10*$scale)){$collapsed=$rect;break}
    }
    if(-not $collapsed){throw ('The floating timer failed to collapse on the '+$edge+' edge. actual='+((Rect-Data $rect)|ConvertTo-Json -Compress))}
    if($collapsed.Left -lt $work.Left -or $collapsed.Top -lt $work.Top -or $collapsed.Right -gt $work.Right -or $collapsed.Bottom -gt $work.Bottom){throw 'The collapsed tab escaped the monitor work area.'}
    [void][LeafDockNative]::SetCursorPos([int](($collapsed.Left+$collapsed.Right)/2),[int](($collapsed.Top+$collapsed.Bottom)/2))
    $expanded=$null
    for($attempt=0;$attempt -lt 20;$attempt++) {Start-Sleep -Milliseconds 120;$rect=Read-Rect $widget;if([Math]::Abs(($rect.Right-$rect.Left)-$width) -le 2 -and [Math]::Abs(($rect.Bottom-$rect.Top)-$height) -le 2){$expanded=$rect;break}}
    if(-not $expanded){throw ('The floating timer failed to expand on the '+$edge+' edge. actual='+((Rect-Data $rect)|ConvertTo-Json -Compress)+' original='+((Rect-Data $originalFrame)|ConvertTo-Json -Compress))}
    $results += [ordered]@{edge=$edge;collapsed=(Rect-Data $collapsed);expanded=(Rect-Data $expanded);passed=$true}
  }
  $report=[ordered]@{version=$appVersion;executable=$executable;testedAt=(Get-Date -Format o);processId=$process.Id;scaleFactor=$scale;mainWindow=(Rect-Data $mainFrame);workArea=(Rect-Data $work);edges=$results;allPassed=$true}
  $report | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $projectRoot 'artifacts\docking-native.json') -Encoding UTF8
  $report | ConvertTo-Json -Depth 7
} finally {
  if($widget -ne [IntPtr]::Zero -and $originalFrame){[void][LeafDockNative]::MoveWindow($widget,$originalFrame.Left,$originalFrame.Top,$originalFrame.Right-$originalFrame.Left,$originalFrame.Bottom-$originalFrame.Top,$true)}
  [void][LeafDockNative]::SetCursorPos($originalPointer.X,$originalPointer.Y)
  if($CloseAfter -and $process -and -not $process.HasExited){Stop-Process -Id $process.Id -Force}
  if($previousContext -ne [IntPtr]::Zero){[void][LeafDockNative]::SetThreadDpiAwarenessContext($previousContext)}
}
