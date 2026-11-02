param([string]$ExecutablePath,[switch]$CloseAfter)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$appVersion=(Get-Content (Join-Path $projectRoot 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
$executable=if($ExecutablePath){[IO.Path]::GetFullPath($ExecutablePath)}else{Join-Path $projectRoot ('artifacts\leafshield-'+$appVersion+'.exe')}
if(Get-Process -Name 'leafshield*' -ErrorAction SilentlyContinue){throw 'Close the existing app before the native startup check.'}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class LeafStartupNative {
  [StructLayout(LayoutKind.Sequential)] public struct RECT {public int Left,Top,Right,Bottom;}
  private delegate bool EnumProc(IntPtr window,IntPtr parameter);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback,IntPtr parameter);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window,out uint processId);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window,StringBuilder text,int count);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out RECT rect);
  public static IntPtr Find(int pid) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((window,parameter)=>{uint owner;GetWindowThreadProcessId(window,out owner);if(owner==pid){var title=new StringBuilder(512);GetWindowText(window,title,512);if(title.ToString().Contains("LeafShield")){found=window;return false;}}return true;},IntPtr.Zero);
    return found;
  }
}
'@
$runPath='Software\Microsoft\Windows\CurrentVersion\Run'
$key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($runPath)
$original=$null
$originalKind=$null
$preferencePath=Join-Path $env:APPDATA 'com.leafshield.desktop\startup.json'
$originalPreference=if(Test-Path -LiteralPath $preferencePath){[IO.File]::ReadAllBytes($preferencePath)}else{$null}
if($key){try{if($key.GetValueNames() -contains 'LeafShield'){$original=$key.GetValue('LeafShield',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames);$originalKind=$key.GetValueKind('LeafShield')}}finally{$key.Dispose()}}
function Read-StartupValue {
  $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($runPath)
  try{if($key){return $key.GetValue('LeafShield',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}}finally{if($key){$key.Dispose()}}
}
function Find-Element($root,[string]$name){
  $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$name)
  $element=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
  if(-not $element){throw ('Missing UI element: '+$name)}
  return $element
}
function Invoke-Button($root,[string]$name){(Find-Element $root $name).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()}
function Startup-Toggle($root){
  $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::CheckBox)
  $checkbox=@($root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition) | Where-Object {$_.Current.Name.StartsWith('开机启动')}) | Select-Object -First 1
  if(-not $checkbox){throw 'The startup switch is missing.'}
  return $checkbox.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
}
$process=$null
$report=$null
try{
  $process=Start-Process -FilePath $executable -ArgumentList '--autostart' -WindowStyle Hidden -PassThru
  $main=[IntPtr]::Zero
  for($attempt=0;$attempt -lt 60;$attempt++){$main=[LeafStartupNative]::Find($process.Id);if($main -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}
  if($main -eq [IntPtr]::Zero){throw 'Background app did not create its hidden main window.'}
  Start-Sleep -Milliseconds 600
  if([LeafStartupNative]::IsWindowVisible($main)){throw 'Autostart unexpectedly showed the main window.'}
  $defaultApplied=$null
  if($null -eq $originalPreference){$defaultApplied=((Read-StartupValue) -eq ('"'+$executable+'" --autostart'));if(-not $defaultApplied){throw 'First launch did not enable startup by default.'}}
  $duplicate=Start-Process -FilePath $executable -ArgumentList '--autostart' -WindowStyle Hidden -PassThru
  if(-not $duplicate.WaitForExit(8000)){throw 'Duplicate startup did not exit.'}
  if([LeafStartupNative]::IsWindowVisible($main)){throw 'Duplicate startup unexpectedly opened the main window.'}
  $manual=Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
  if(-not $manual.WaitForExit(8000)){throw 'Manual duplicate did not exit.'}
  for($attempt=0;$attempt -lt 30;$attempt++){if([LeafStartupNative]::IsWindowVisible($main)){break};Start-Sleep -Milliseconds 100}
  if(-not [LeafStartupNative]::IsWindowVisible($main)){throw 'Manual launch did not open the running app.'}
  $root=[System.Windows.Automation.AutomationElement]::FromHandle($main)
  for($attempt=0;$attempt -lt 40;$attempt++){
    try{Invoke-Button $root '选项';break}catch{if($attempt -eq 39){throw};Start-Sleep -Milliseconds 100}
  }
  Start-Sleep -Milliseconds 400
  $toggle=Startup-Toggle $root
  if($toggle.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On){$toggle.Toggle();Start-Sleep -Milliseconds 300}
  $toggle.Toggle()
  $expected='"'+$executable+'" --autostart'
  for($attempt=0;$attempt -lt 30;$attempt++){if((Read-StartupValue) -eq $expected){break};Start-Sleep -Milliseconds 100}
  if((Read-StartupValue) -ne $expected){throw 'The real startup switch did not save the quoted executable command.'}
  Invoke-Button $root '计时器'
  Invoke-Button $root '选项'
  Start-Sleep -Milliseconds 300
  $toggle=Startup-Toggle $root
  if($toggle.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On){throw 'Startup state did not reload from the registry.'}
  $toggle.Toggle()
  for($attempt=0;$attempt -lt 30;$attempt++){if($null -eq (Read-StartupValue)){break};Start-Sleep -Milliseconds 100}
  if($null -ne (Read-StartupValue)){throw 'Disabling startup did not remove the app entry.'}
  if((Get-Content $preferencePath -Raw -Encoding UTF8 | ConvertFrom-Json) -ne $false){throw 'Explicit off choice was not persisted.'}
  Stop-Process -Id $process.Id -Force
  $process.WaitForExit()
  $process=Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
  for($attempt=0;$attempt -lt 60;$attempt++){$main=[LeafStartupNative]::Find($process.Id);if($main -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}
  Start-Sleep -Milliseconds 500
  if($null -ne (Read-StartupValue)){throw 'Restart unexpectedly re-enabled an explicit off choice.'}
  $root=[System.Windows.Automation.AutomationElement]::FromHandle($main)
  $report=[ordered]@{version=$appVersion;executable=$executable;processId=$process.Id;checkedAt=(Get-Date -Format o);backgroundMainHidden=$true;duplicateStartupStayedHidden=$true;manualLaunchOpenedMain=$true;registryCommandMatched=$true;stateReloaded=$true;disableRemovedEntry=$true;explicitOffSurvivedRestart=$true;originalEntryPresent=($null -ne $original);allPassed=$true;mode='simulated-login-arguments-no-real-reboot'}
  $report.defaultStartupApplied=$defaultApplied
}finally{
  # Restore exactly the one value that this check owns; keep every other startup entry.
  $key=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($runPath)
  try{if($null -ne $original){$key.SetValue('LeafShield',$original,$originalKind)}else{$key.DeleteValue('LeafShield',$false)}}finally{$key.Dispose()}
  if($null -ne $originalPreference){[IO.File]::WriteAllBytes($preferencePath,$originalPreference)}elseif(Test-Path -LiteralPath $preferencePath){Remove-Item -LiteralPath $preferencePath}
  if($root){try{Invoke-Button $root '计时器';Invoke-Button $root '选项';Start-Sleep -Milliseconds 300}catch{Write-Warning $_}}
}
if($report){
  $report.originalEntryRestored=((Read-StartupValue) -eq $original)
  if(-not $report.originalEntryRestored){throw 'Original startup entry was not restored.'}
  [void][LeafStartupNative]::SetForegroundWindow($main)
  $report.screenshotCaptured=([LeafStartupNative]::GetForegroundWindow() -eq $main)
  if($report.screenshotCaptured){
    $rect=New-Object LeafStartupNative+RECT
    [void][LeafStartupNative]::GetWindowRect($main,[ref]$rect)
    $bitmap=New-Object Drawing.Bitmap(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top))
    $graphics=[Drawing.Graphics]::FromImage($bitmap)
    try{$graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bitmap.Size);$bitmap.Save((Join-Path $projectRoot 'artifacts\native-startup-options.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
  }
  $report | ConvertTo-Json | Set-Content (Join-Path $projectRoot 'artifacts\startup-native.json') -Encoding UTF8
  $process.Id | Set-Content (Join-Path $projectRoot 'artifacts\running-pid.txt') -Encoding ASCII
  $report | ConvertTo-Json
  if($CloseAfter -and $process -and -not $process.HasExited){Stop-Process -Id $process.Id -Force}
}
