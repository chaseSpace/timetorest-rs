$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$version=(Get-Content (Join-Path $projectRoot 'package.json') -Raw -Encoding UTF8|ConvertFrom-Json).version
$executable=Join-Path $projectRoot ('artifacts\leafshield-'+$version+'.exe')
if(Get-Process -Name 'leafshield*' -ErrorAction SilentlyContinue){throw 'Close the existing app before this colour check.'}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
Add-Type -TypeDefinition @'
using System;using System.Text;using System.Runtime.InteropServices;
public static class LeafRgbNative {
 [StructLayout(LayoutKind.Sequential)] public struct Effect {[MarshalAs(UnmanagedType.ByValArray,SizeConst=25)] public float[] data;}
 private delegate bool EnumProc(IntPtr window,IntPtr parameter);
 [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback,IntPtr parameter);
 [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window,StringBuilder text,int count);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window,StringBuilder text,int count);
 [DllImport("Magnification.dll")] public static extern bool MagInitialize();
 [DllImport("Magnification.dll")] public static extern bool MagUninitialize();
 [DllImport("Magnification.dll")] public static extern bool MagGetFullscreenColorEffect(ref Effect effect);
 public static IntPtr FindMain(int pid){IntPtr found=IntPtr.Zero;EnumWindows((window,p)=>{uint owner;GetWindowThreadProcessId(window,out owner);if(owner==pid){var text=new StringBuilder(512);GetWindowText(window,text,512);if(text.ToString().Contains("LeafShield")){found=window;return false;}}return true;},IntPtr.Zero);return found;}
 public static int OverlayCount(int pid){int count=0;EnumWindows((window,p)=>{uint owner;GetWindowThreadProcessId(window,out owner);if(owner==pid){var text=new StringBuilder(256);GetClassName(window,text,256);if(text.ToString()=="LeafShieldDisplayFilter")count++;}return true;},IntPtr.Zero);return count;}
}
'@
$config=Join-Path $env:APPDATA 'com.leafshield.desktop\settings.json'
$old=(Get-Content $config -Raw -Encoding UTF8|ConvertFrom-Json).settings
function Find-Name($root,[string]$name){$condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$name);$value=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition);if(-not $value){throw ('Missing UI element: '+$name)};return $value}
function Click([string]$name){(Find-Name $root $name).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()}
function Set-Range([string]$name,[int]$value){(Find-Name $root $name).GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).SetValue($value)}
function Toggle-Display {
 $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::CheckBox)
 $element=@($root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)|Where-Object {$_.Current.Name.StartsWith('启用屏幕调节')})|Select-Object -First 1
 if(-not $element){throw 'Missing display toggle'};$element.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
}
function Read-Matrix {
 $effect=New-Object LeafRgbNative+Effect;$effect.data=New-Object single[] 25
 if(-not [LeafRgbNative]::MagGetFullscreenColorEffect([ref]$effect)){throw 'Could not read system colour matrix'}
 return ,$effect.data
}
function Read-Document {
 $stream=[IO.File]::Open($config,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 $reader=New-Object IO.StreamReader($stream,[Text.Encoding]::UTF8)
 try{return ($reader.ReadToEnd()|ConvertFrom-Json)}finally{$reader.Dispose();$stream.Dispose()}
}
function Read-Settings {return (Read-Document).settings}
function Wait-Settings([scriptblock]$condition){for($attempt=0;$attempt -lt 40;$attempt++){if(& $condition (Read-Settings)){return};Start-Sleep -Milliseconds 100};throw ('Display settings did not reach the expected state: '+$stage+' '+((Read-Settings)|Select-Object temperatureKelvin,brightnessPercent,displayEnabled|ConvertTo-Json -Compress))}
function Wait-Stable {
 $last=Read-Matrix;$steady=0
 for($attempt=0;$attempt -lt 60;$attempt++){Start-Sleep -Milliseconds 25;$current=Read-Matrix;$delta=[Math]::Max([Math]::Abs($last[0]-$current[0]),[Math]::Max([Math]::Abs($last[6]-$current[6]),[Math]::Abs($last[12]-$current[12])));if($delta -lt 0.00001){$steady++}else{$steady=0};if($steady -ge 4){return ,$current};$last=$current}
 throw 'Colour transition did not settle.'
}
$process=$null;$root=$null;$initialised=$false;$passed=$false
try{
 $process=Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
 $main=[IntPtr]::Zero
 for($attempt=0;$attempt -lt 80;$attempt++){$main=[LeafRgbNative]::FindMain($process.Id);if($main -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}
 if($main -eq [IntPtr]::Zero){throw 'Main window did not appear.'}
 $root=[System.Windows.Automation.AutomationElement]::FromHandle($main)
 for($attempt=0;$attempt -lt 50;$attempt++){try{[void](Find-Name $root '屏幕色温');break}catch{if($attempt -eq 49){throw};Start-Sleep -Milliseconds 100}}
 $initialised=[LeafRgbNative]::MagInitialize();if(-not $initialised){throw 'Observer initialization failed.'}
 $stage='enable';if(-not $old.displayEnabled){Toggle-Display;Wait-Settings {param($s) $s.displayEnabled}}
 $stage='neutral';Click '恢复原色';Wait-Settings {param($s) $s.temperatureKelvin -eq 6500 -and $s.brightnessPercent -eq 100};$neutral=Wait-Stable
 if([Math]::Abs($neutral[0]-1)-gt 0.0001 -or [Math]::Abs($neutral[6]-1)-gt 0.0001 -or [Math]::Abs($neutral[12]-1)-gt 0.0001){throw 'Original colours did not restore.'}
 $stage='brightness';Set-Range '屏幕亮度' 80;Wait-Settings {param($s) $s.brightnessPercent -eq 80};[void](Wait-Stable)
 $stage='warm';Set-Range '屏幕色温' 5000;Wait-Settings {param($s) $s.temperatureKelvin -eq 5000};$first=Wait-Stable
 if([Math]::Abs($first[0]-0.8)-gt 0.0001 -or $first[6] -ge $first[0] -or $first[12] -ge $first[6]){throw 'Warm RGB channel gains did not apply.'}
 Set-Range '屏幕色温' 3500
 $samples=@()
 for($frame=0;$frame -lt 40;$frame++){$sample=Read-Matrix;$samples+=,@($sample[0],$sample[6],$sample[12]);if([Math]::Abs($sample[0]-0.8)-gt 0.001){throw 'Warm adjustment passed through a neutral/bright frame.'};foreach($offset in @(20,21,22)){if([Math]::Abs($sample[$offset])-gt 0.00001){throw 'The colour transform lifted black pixels.'}};Start-Sleep -Milliseconds 10}
 Wait-Settings {param($s) $s.temperatureKelvin -eq 3500};$second=Wait-Stable
 if($second[6] -ge $first[6] -or $second[12] -ge $first[12]){throw 'Lower temperature did not reduce green/blue continuously.'}
 $overlays=[LeafRgbNative]::OverlayCount($process.Id);if($overlays -ne 0){throw 'Legacy overlay windows remain.'}
 $report=[ordered]@{version=$version;executable=$executable;processId=$process.Id;checkedAt=(Get-Date -Format o);backend='system-rgb';rgb5000K=@($first[0],$first[6],$first[12]);rgb3500K=@($second[0],$second[6],$second[12]);transitionSamples=$samples;noNeutralOrBrightFrame=$true;blackOffsetsZero=$true;overlayWindows=$overlays;neutralRestored=$true}
 $passed=$true
}finally{
 if(-not $passed -and $root -and $process -and -not $process.HasExited){try{Set-Range '屏幕色温' $old.temperatureKelvin;Set-Range '屏幕亮度' $old.brightnessPercent;Wait-Settings {param($s) $s.temperatureKelvin -eq $old.temperatureKelvin -and $s.brightnessPercent -eq $old.brightnessPercent};if(-not $old.displayEnabled){Toggle-Display};[void](Wait-Stable)}catch{Write-Warning $_}}
 if(-not $passed -and $initialised){[void][LeafRgbNative]::MagUninitialize();$initialised=$false}
}
if($passed){
 Stop-Process -Id $process.Id -Force;$process.WaitForExit();Start-Sleep -Milliseconds 250
 $after=Read-Matrix
 $report.processExitRestored=([Math]::Abs($after[0]-1)-lt 0.0001 -and [Math]::Abs($after[6]-1)-lt 0.0001 -and [Math]::Abs($after[12]-1)-lt 0.0001)
 [void][LeafRgbNative]::MagUninitialize();$initialised=$false
 $document=Read-Document
 $document.settings.temperatureKelvin=$old.temperatureKelvin
 $document.settings.brightnessPercent=$old.brightnessPercent
 $document.settings.displayEnabled=$old.displayEnabled
 $temporary=$config+'.colour-restore.tmp'
 [IO.File]::WriteAllText($temporary,($document|ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
 [IO.File]::Replace($temporary,$config,[NullString]::Value)
 if(-not $report.processExitRestored){throw 'System colour effect survived its owner process.'}
 $report.originalSettingsRestored=$true;$report.allPassed=$true
 $report|ConvertTo-Json -Depth 6|Set-Content (Join-Path $projectRoot 'artifacts\colour-native.json') -Encoding UTF8
 $final=Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
 $final.Id|Set-Content (Join-Path $projectRoot 'artifacts\running-pid.txt') -Encoding ASCII
 [pscustomobject]$report|Select-Object version,processId,backend,rgb5000K,rgb3500K,noNeutralOrBrightFrame,blackOffsetsZero,overlayWindows,neutralRestored,processExitRestored,allPassed|ConvertTo-Json -Depth 4
}
