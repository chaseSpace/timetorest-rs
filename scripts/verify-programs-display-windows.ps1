param([int]$AppProcessId=0)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$version=(Get-Content (Join-Path $projectRoot 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
if($version -ne '0.4.0'){throw 'This legacy overlay check targets 0.4.0. Use verify-colour-windows.ps1 for the RGB engine.'}
$executable=Join-Path $projectRoot ('artifacts\leafshield-'+$version+'.exe')
if(-not $AppProcessId -and (Get-Process -Name 'leafshield*' -ErrorAction SilentlyContinue)){throw 'Close the existing app before this native check.'}
if($AppProcessId -and (Get-Process -Id $AppProcessId).Path -ne $executable){throw 'The requested process is not this release.'}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class LeafFeatureNative {
  [StructLayout(LayoutKind.Sequential)] public struct RECT {public int Left,Top,Right,Bottom;}
  private delegate bool EnumProc(IntPtr window,IntPtr parameter);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback,IntPtr parameter);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window,out uint processId);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window,StringBuilder text,int count);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window,StringBuilder text,int count);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out RECT rect);
  [DllImport("user32.dll",EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr window,int index);
  [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr window,out uint colour,out byte alpha,out uint flags);
  [DllImport("user32.dll")] private static extern IntPtr GetDlgItem(IntPtr window,int id);
  [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr window,EnumProc callback,IntPtr parameter);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern bool SetWindowText(IntPtr window,string text);
  [DllImport("user32.dll",CharSet=CharSet.Unicode,EntryPoint="SendMessageW")] private static extern IntPtr SendText(IntPtr window,uint message,IntPtr w,string text);
  [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window,uint message,IntPtr w,IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  public static IntPtr[] Find(int pid,string windowClass,string titlePart) {
    var found=new List<IntPtr>();
    EnumWindows((window,parameter)=>{uint owner;GetWindowThreadProcessId(window,out owner);if(owner==pid){var title=new StringBuilder(512);GetWindowText(window,title,512);var cls=new StringBuilder(256);GetClassName(window,cls,256);if((String.IsNullOrEmpty(windowClass)||cls.ToString()==windowClass)&&(String.IsNullOrEmpty(titlePart)||title.ToString().Contains(titlePart)))found.Add(window);}return true;},IntPtr.Zero);
    return found.ToArray();
  }
  public static string Title(IntPtr window){var text=new StringBuilder(512);GetWindowText(window,text,512);return text.ToString();}
  public static bool DialogClick(IntPtr dialog,int id){var button=GetDlgItem(dialog,id);return button!=IntPtr.Zero&&PostMessage(dialog,0x0111,new IntPtr(id),button);}
  public static bool DialogFilename(IntPtr dialog,string path){
    var host=GetDlgItem(dialog,1148);if(host==IntPtr.Zero)return false;
    IntPtr edit=IntPtr.Zero;EnumChildWindows(host,(child,parameter)=>{var cls=new StringBuilder(128);GetClassName(child,cls,128);if(cls.ToString()=="Edit"){edit=child;return false;}return true;},IntPtr.Zero);
    return SendText(edit==IntPtr.Zero?host:edit,0x000C,IntPtr.Zero,path)!=IntPtr.Zero;
  }
}
'@
$context=[LeafFeatureNative]::SetThreadDpiAwarenessContext([IntPtr]::new(-4))
$config=Join-Path $env:APPDATA 'com.leafshield.desktop\settings.json'
$old=(Get-Content $config -Raw -Encoding UTF8 | ConvertFrom-Json).settings
$oldWhitelist=@($old.whitelist)
$oldTemperature=if($null -ne $old.temperatureKelvin){[int]$old.temperatureKelvin}else{5000}
$oldBrightness=if($null -ne $old.brightnessPercent){[int]$old.brightnessPercent}else{80}
$oldEnabled=if($null -ne $old.displayEnabled){[bool]$old.displayEnabled}else{$true}
$restoreFile=Join-Path $projectRoot 'artifacts\native-feature-restore.json'
if(Test-Path -LiteralPath $restoreFile){$restore=Get-Content $restoreFile -Raw -Encoding UTF8|ConvertFrom-Json;$oldWhitelist=@($restore.whitelist);$oldTemperature=[int]$restore.temperature;$oldBrightness=[int]$restore.brightness;$oldEnabled=[bool]$restore.enabled}else{[ordered]@{whitelist=$oldWhitelist;temperature=$oldTemperature;brightness=$oldBrightness;enabled=$oldEnabled}|ConvertTo-Json|Set-Content $restoreFile -Encoding UTF8}
$fixtureRoot=Join-Path $projectRoot ('artifacts\whitelist-fixture-'+[Guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $fixtureRoot)
$shortcutPath=Join-Path $fixtureRoot '桌面测试快捷方式.lnk'
$shell=New-Object -ComObject WScript.Shell
$shortcut=$shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath=Join-Path $env:WINDIR 'System32\cmd.exe'
$shortcut.Arguments='/c exit'
$shortcut.Save()
function Find-Name($root,[string]$name){$condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$name);$element=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition);if(-not $element){throw ('Missing UI element: '+$name)};return $element}
function Click($root,[string]$name){for($retry=0;$retry -lt 30;$retry++){try{(Find-Name $root $name).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke();return}catch{if($retry -eq 29){throw};$root=[System.Windows.Automation.AutomationElement]::FromHandle($main);Start-Sleep -Milliseconds 100}}}
function Set-Range($root,[string]$name,[int]$value){(Find-Name $root $name).GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).SetValue($value)}
function Read-Settings{return (Get-Content $config -Raw -Encoding UTF8 | ConvertFrom-Json).settings}
function Wait-Settings([scriptblock]$condition){for($attempt=0;$attempt -lt 40;$attempt++){if(& $condition (Read-Settings)){return};Start-Sleep -Milliseconds 100};throw 'Settings did not reach the expected state.'}
function Whitelist-Input($root){$condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Edit);$element=@($root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)|Where-Object {$_.Current.AutomationId -eq 'whitelist' -or $_.Current.Name.StartsWith('前台程序白名单')})|Select-Object -First 1;if(-not $element){throw 'Whitelist text input missing.'};return $element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)}
function Dialog-Button($dialog,[string]$id){if(-not [LeafFeatureNative]::DialogClick([IntPtr]::new($dialog.Current.NativeWindowHandle),[int]$id)){throw ('Dialog button missing: '+$id)}}
function Choose-File([string]$path,[switch]$Cancel){
  Click $root '选择程序'
  $dialogHandle=[IntPtr]::Zero
  for($attempt=0;$attempt -lt 50;$attempt++){$items=@([LeafFeatureNative]::Find($process.Id,'#32770',$null)|Where-Object {[LeafFeatureNative]::IsWindowVisible($_)});if($items.Count){$dialogHandle=$items[0];break};Start-Sleep -Milliseconds 100}
  if($dialogHandle -eq [IntPtr]::Zero){throw 'The native program picker did not appear.'}
  $dialog=[System.Windows.Automation.AutomationElement]::FromHandle($dialogHandle)
  if($Cancel){Dialog-Button $dialog '2'}else{
    $ready=$false
    for($attempt=0;$attempt -lt 30;$attempt++){if([LeafFeatureNative]::DialogFilename($dialogHandle,$path)){$ready=$true;break};Start-Sleep -Milliseconds 100}
    if(-not $ready){throw 'Native picker filename input missing.'}
    Dialog-Button $dialog '1'
  }
  for($attempt=0;$attempt -lt 50;$attempt++){if(-not [LeafFeatureNative]::IsWindowVisible($dialogHandle)){Start-Sleep -Milliseconds 200;return};Start-Sleep -Milliseconds 100}
  throw 'Native program picker did not close.'
}
function Filter-Data {
  $result=@()
  foreach($window in [LeafFeatureNative]::Find($process.Id,'LeafShieldDisplayFilter',$null)){
    $rect=New-Object LeafFeatureNative+RECT;[void][LeafFeatureNative]::GetWindowRect($window,[ref]$rect)
    [uint32]$colour=0;[byte]$alpha=0;[uint32]$flags=0;[void][LeafFeatureNative]::GetLayeredWindowAttributes($window,[ref]$colour,[ref]$alpha,[ref]$flags)
    $style=[LeafFeatureNative]::GetWindowLongPtr($window,-20).ToInt64()
    $result += [ordered]@{title=[LeafFeatureNative]::Title($window);visible=[LeafFeatureNative]::IsWindowVisible($window);alpha=$alpha;x=$rect.Left;y=$rect.Top;width=$rect.Right-$rect.Left;height=$rect.Bottom-$rect.Top;clickThrough=(($style -band 0x20) -ne 0);noActivate=(($style -band 0x08000000) -ne 0)}
  }
  return $result
}
$process=$null;$root=$null;$passed=$false
try{
  $process=if($AppProcessId){Get-Process -Id $AppProcessId}else{Start-Process -FilePath $executable -WindowStyle Hidden -PassThru}
  $main=[IntPtr]::Zero
  for($attempt=0;$attempt -lt 70;$attempt++){$items=[LeafFeatureNative]::Find($process.Id,$null,'LeafShield');if($items.Count){$main=$items[0];break};Start-Sleep -Milliseconds 100}
  if($main -eq [IntPtr]::Zero){throw 'The main window did not appear.'}
  $root=[System.Windows.Automation.AutomationElement]::FromHandle($main)
  foreach($existing in [LeafFeatureNative]::Find($process.Id,'#32770',$null)){[void][LeafFeatureNative]::DialogClick($existing,2)}
  Start-Sleep -Milliseconds 250
  for($attempt=0;$attempt -lt 50;$attempt++){try{[void](Find-Name $root '屏幕色温');break}catch{if($attempt -eq 49){throw};Start-Sleep -Milliseconds 100}}
  if(-not $oldEnabled){$check=Find-Name $root '启用屏幕调节';$check.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()}
  Set-Range $root '屏幕色温' 3500
  Set-Range $root '屏幕亮度' 65
  Wait-Settings {param($s) $s.temperatureKelvin -eq 3500 -and $s.brightnessPercent -eq 65}
  Start-Sleep -Milliseconds 500
  $filters=@(Filter-Data)
  if($filters.Count -lt 2 -or $filters.Count % 2 -ne 0){throw 'Monitor filters were not created.'}
  foreach($filter in $filters){if(-not $filter.visible -or -not $filter.clickThrough -or -not $filter.noActivate){throw 'A filter did not remain visible and click-through.'};$expected=if($filter.title.Contains('色温')){58}else{89};if($filter.alpha -ne $expected){throw 'Slider values did not reach the native filters.'}}
  Click $root '恢复原色'
  Wait-Settings {param($s) $s.temperatureKelvin -eq 6500 -and $s.brightnessPercent -eq 100}
  Start-Sleep -Milliseconds 400
  if(@(Filter-Data|Where-Object {$_.visible}).Count){throw 'Neutral values left a visible filter.'}
  Click $root '选项'
  (Whitelist-Input $root).SetValue('')
  Click $root '保存白名单'
  Wait-Settings {param($s) @($s.whitelist).Count -eq 0}
  Choose-File (Join-Path $env:WINDIR 'System32\notepad.exe')
  Wait-Settings {param($s) @($s.whitelist).Count -eq 1 -and $s.whitelist[0] -eq 'notepad.exe'}
  Choose-File $shortcutPath
  Wait-Settings {param($s) @($s.whitelist).Count -eq 2 -and $s.whitelist -contains 'cmd.exe'}
  Choose-File (Join-Path $env:WINDIR 'System32\notepad.exe')
  Wait-Settings {param($s) @($s.whitelist).Count -eq 2}
  Choose-File '' -Cancel
  if(@((Read-Settings).whitelist).Count -ne 2){throw 'Cancelling changed the whitelist.'}
  $report=[ordered]@{version=$version;executable=$executable;processId=$process.Id;checkedAt=(Get-Date -Format o);exePickerAdded=$true;shortcutResolvedToTarget=$true;duplicateIgnored=$true;cancelPreservedList=$true;monitorCount=$filters.Count/2;nativeFilters=$filters;slidersApplied=$true;neutralHidFilters=$true}
  $passed=$true
}finally{
  if($process -and -not $process.HasExited){
    foreach($dialog in [LeafFeatureNative]::Find($process.Id,'#32770',$null)){try{Dialog-Button ([System.Windows.Automation.AutomationElement]::FromHandle($dialog)) '2'}catch{Write-Warning $_}}
    if($root){
      try{
        Click $root '选项';(Whitelist-Input $root).SetValue(($oldWhitelist -join "`n"));Click $root '保存白名单'
        Wait-Settings {param($s) (($s.whitelist -join "`n") -eq ($oldWhitelist -join "`n"))}
        Click $root '显示器';Set-Range $root '屏幕色温' $oldTemperature;Set-Range $root '屏幕亮度' $oldBrightness
        Wait-Settings {param($s) $s.temperatureKelvin -eq $oldTemperature -and $s.brightnessPercent -eq $oldBrightness}
        if(-not $oldEnabled){(Find-Name $root '启用屏幕调节').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()}
      }catch{Write-Warning $_;$passed=$false}
    }
  }
  $resolved=[IO.Path]::GetFullPath($fixtureRoot)
  if(-not $resolved.StartsWith([IO.Path]::GetFullPath((Join-Path $projectRoot 'artifacts'))+[IO.Path]::DirectorySeparatorChar)){throw 'Fixture cleanup target escaped the workspace.'}
  Remove-Item -LiteralPath $shortcutPath -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $fixtureRoot -ErrorAction SilentlyContinue
  if($context -ne [IntPtr]::Zero){[void][LeafFeatureNative]::SetThreadDpiAwarenessContext($context)}
}
if($passed){
  $handles=[LeafFeatureNative]::Find($process.Id,'LeafShieldDisplayFilter',$null)
  Stop-Process -Id $process.Id -Force;$process.WaitForExit()
  if(@($handles|Where-Object {[LeafFeatureNative]::IsWindow($_)}).Count){throw 'Filter windows survived their process.'}
  $report.processExitRemovedFilters=$true;$report.originalSettingsRestored=$true;$report.allPassed=$true
  $report|ConvertTo-Json -Depth 6|Set-Content (Join-Path $projectRoot 'artifacts\programs-display-native.json') -Encoding UTF8
  $final=Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
  $final.Id|Set-Content (Join-Path $projectRoot 'artifacts\running-pid.txt') -Encoding ASCII
  $report|ConvertTo-Json -Depth 6
  Remove-Item -LiteralPath $restoreFile
}
