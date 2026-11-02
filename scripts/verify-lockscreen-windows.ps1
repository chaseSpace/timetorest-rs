param([Parameter(Mandatory=$true)][int]$AppProcessId)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class LeafPhotoNative {
  [StructLayout(LayoutKind.Sequential)] public struct RECT {public int Left,Top,Right,Bottom;}
  private delegate bool EnumProc(IntPtr window,IntPtr parameter);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback,IntPtr parameter);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window,out uint processId);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window,StringBuilder text,int count);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window,int command);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out RECT rect);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  public static IntPtr Find(int pid,string titlePart) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((window,parameter)=>{uint owner;GetWindowThreadProcessId(window,out owner);if(owner==pid){var title=new StringBuilder(512);GetWindowText(window,title,512);if(title.ToString().Contains(titlePart)){found=window;return false;}}return true;},IntPtr.Zero);
    return found;
  }
}
'@
$projectRoot=Split-Path -Parent $PSScriptRoot
$process=Get-Process -Id $AppProcessId
$expected=[IO.Path]::GetFullPath((Join-Path $projectRoot ('artifacts\leafshield-'+(Get-Content (Join-Path $projectRoot 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version+'.exe')))
if($process.Path -ne $expected){throw 'The requested process is not this project release.'}
$settingsFile=Join-Path $env:APPDATA 'com.leafshield.desktop\settings.json'
$before=(Get-Content $settingsFile -Raw -Encoding UTF8 | ConvertFrom-Json).settings
$previousContext=[LeafPhotoNative]::SetThreadDpiAwarenessContext([IntPtr]::new(-4))
function Find-Button($root,[string]$name) {
  $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$name)
  $button=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
  if(-not $button){throw ('Missing native UI element: '+$name)}
  return $button
}
function Invoke-Button($element){$element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()}
$preview=[IntPtr]::Zero
try {
  $main=[LeafPhotoNative]::Find($AppProcessId,'LeafShield')
  if($main -eq [IntPtr]::Zero){throw 'The compact main window is missing.'}
  [void][LeafPhotoNative]::ShowWindow($main,9)
  [void][LeafPhotoNative]::SetForegroundWindow($main)
  Start-Sleep -Milliseconds 500
  $root=[System.Windows.Automation.AutomationElement]::FromHandle($main)
  Invoke-Button (Find-Button $root '计时器')
  foreach($rhythm in @('25 / 2','45 / 2','60 / 3','90 / 5')){[void](Find-Button $root $rhythm)}
  Invoke-Button (Find-Button $root '选项')
  Start-Sleep -Milliseconds 250
  Invoke-Button (Find-Button $root '预览')
  for($attempt=0;$attempt -lt 60;$attempt++){$preview=[LeafPhotoNative]::Find($AppProcessId,'休息预览');if($preview -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}
  if($preview -eq [IntPtr]::Zero){throw 'The new photo preview did not appear.'}
  [void][LeafPhotoNative]::SetForegroundWindow($preview)
  Start-Sleep -Milliseconds 1500
  $screen=[System.Windows.Automation.AutomationElement]::FromHandle($preview)
  [void](Find-Button $screen '休息一下')
  [void](Find-Button $screen '手动输入正念文案')
  $elements=$screen.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
  $scrollbars=@($elements | Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::ScrollBar -and -not $_.Current.IsOffscreen})
  if($scrollbars.Count){throw 'A visible scrollbar remains on the new lock screen.'}
  $rect=New-Object LeafPhotoNative+RECT
  [void][LeafPhotoNative]::GetWindowRect($preview,[ref]$rect)
  $bitmap=New-Object Drawing.Bitmap(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top))
  $graphics=[Drawing.Graphics]::FromImage($bitmap)
  try {$graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bitmap.Size);$bitmap.Save((Join-Path $projectRoot 'artifacts\native-photo-preview.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
  $report=[ordered]@{executable=$process.Path;processId=$AppProcessId;checkedAt=(Get-Date -Format o);theme=$before.theme;rhythmPresets=@('25/2','45/2','60/3','90/5');visiblePreviewScrollbars=$scrollbars.Count;newTitleRendered=$true;unlockInputPresent=$true;screenshot='artifacts/native-photo-preview.png';mode='native-preview-no-lock'}
  $report | ConvertTo-Json | Set-Content (Join-Path $projectRoot 'artifacts\native-photo-ui.json') -Encoding UTF8
  $report | ConvertTo-Json
}finally{
  if($preview -ne [IntPtr]::Zero){
    try {$screen=[System.Windows.Automation.AutomationElement]::FromHandle($preview);Invoke-Button (Find-Button $screen '关闭预览 ×')}catch{Write-Warning $_}
  }
  if($root){try{Invoke-Button (Find-Button $root '计时器')}catch{Write-Warning $_}}
  if($previousContext -ne [IntPtr]::Zero){[void][LeafPhotoNative]::SetThreadDpiAwarenessContext($previousContext)}
}
