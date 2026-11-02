param([string]$Executable = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Executable) {
    $manifest = Get-Content -LiteralPath (Join-Path $projectRoot 'crates\leafshield-native\Cargo.toml') -Raw -Encoding UTF8
    $nativeVersion = [regex]::Match($manifest, '(?m)^version = "([^"]+)"').Groups[1].Value
    $Executable = Join-Path $projectRoot ('artifacts\native-dev\leafshield-native-' + $nativeVersion + '.exe')
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$testDirectory = Join-Path $projectRoot ('artifacts\native-dev\ui-test-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
$profileDirectory = Join-Path $testDirectory 'profile'
[IO.Directory]::CreateDirectory($profileDirectory) > $null
# Rust JSON expects UTF-8 without the Windows PowerShell BOM.
[IO.File]::WriteAllText((Join-Path $profileDirectory 'settings.json'), '{"settings":{"workMinutes":240,"restMinutes":2,"displayEnabled":false,"floating":true,"notifications":false}}', [Text.UTF8Encoding]::new($false))
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
public static class LeafShieldUiTest {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public delegate bool EnumWindow(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindow callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr window, int index);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr parent, int id);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr window, IntPtr rect, IntPtr region, uint flags);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    [DllImport("gdi32.dll")] public static extern IntPtr CreateCompatibleDC(IntPtr dc);
    [DllImport("gdi32.dll")] public static extern bool DeleteDC(IntPtr dc);
    [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr obj);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateFontW(int height, int width, int escapement, int orientation, int weight, uint italic, uint underline, uint strikeout, uint charset, uint output, uint clip, uint quality, uint pitch, string family);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int DrawTextW(IntPtr dc, string text, int count, ref Rect rect, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageW(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool PostMessageW(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr GetPropW(IntPtr window, string name);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr menu, int index);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuStringW(IntPtr menu, uint index, StringBuilder text, int count, uint flags);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    public static IntPtr Find(int process, int role) {
        IntPtr found = IntPtr.Zero;
        EnumWindows(delegate(IntPtr window, IntPtr data) {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner == process && GetWindowLongPtr(window, -21).ToInt64() == role) { found = window; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static string Text(IntPtr window) { var text = new StringBuilder(1024); GetWindowTextW(window, text, text.Capacity); return text.ToString(); }
    public static bool TextFits(IntPtr window) {
        Rect bounds; GetClientRect(window, out bounds); int available = bounds.Bottom;
        IntPtr dc = CreateCompatibleDC(IntPtr.Zero);
        IntPtr font = CreateFontW(-(int)Math.Round(14 * GetDpiForWindow(window) / 96.0), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, "Segoe UI");
        IntPtr previous = SelectObject(dc, font);
        try { DrawTextW(dc, Text(window), -1, ref bounds, 0x410); return bounds.Bottom <= available; }
        finally { SelectObject(dc, previous); DeleteObject(font); DeleteDC(dc); }
    }
    public static void Click(IntPtr window, int x, int y) {
        var point = new IntPtr((x & 65535) | ((y & 65535) << 16));
        SendMessageW(window, 0x201, new IntPtr(1), point); SendMessageW(window, 0x202, IntPtr.Zero, point);
    }
    public static void Drag(IntPtr window, int from, int to, int y) {
        var start = new IntPtr((from & 65535) | ((y & 65535) << 16));
        var end = new IntPtr((to & 65535) | ((y & 65535) << 16));
        SendMessageW(window, 0x201, new IntPtr(1), start);
        SendMessageW(window, 0x200, new IntPtr(1), end);
        SendMessageW(window, 0x202, IntPtr.Zero, end);
    }
    public static string[] Menu(IntPtr menu) {
        var result = new List<string>();
        for (int index = 0; index < GetMenuItemCount(menu); index++) {
            var text = new StringBuilder(256); GetMenuStringW(menu, (uint)index, text, text.Capacity, 0x400);
            result.Add(GetMenuItemID(menu, index).ToString() + ":" + text.ToString());
        }
        return result.ToArray();
    }
    public static int[] Capture(IntPtr window, string path) {
        Rect bounds; GetWindowRect(window, out bounds);
        int width = bounds.Right - bounds.Left, height = bounds.Bottom - bounds.Top;
        using (var bitmap = new Bitmap(width, height, PixelFormat.Format32bppArgb)) {
            using (var graphics = Graphics.FromImage(bitmap)) {
                IntPtr dc = graphics.GetHdc();
                try { if (!PrintWindow(window, dc, 2)) throw new Exception("PrintWindow failed"); }
                finally { graphics.ReleaseHdc(dc); }
            }
            bitmap.Save(path, ImageFormat.Png);
            int blue = 0, maxColumn = 0; var peaks = new List<int>();
            for (int x = 0; x < width; x++) {
                int column = 0;
                for (int y = 0; y < height; y++) {
                    Color pixel = bitmap.GetPixel(x, y);
                    if (pixel.R == 0 && pixel.G == 103 && pixel.B == 192) { column++; blue++; }
                }
                if (column > maxColumn) { maxColumn = column; peaks.Clear(); }
                if (column == maxColumn) peaks.Add(x);
            }
            int center = peaks.Count == 0 ? -1 : peaks[peaks.Count / 2];
            return new int[] { width, height, blue, maxColumn, center };
        }
    }
    public static int VisibleRedrawDifference(IntPtr window, string prefix) {
        Rect rect; GetWindowRect(window, out rect);
        int width = rect.Right - rect.Left, height = rect.Bottom - rect.Top;
        using (var before = new Bitmap(width, height)) using (var after = new Bitmap(width, height)) {
            DwmFlush();
            using (var graphics = Graphics.FromImage(before)) { graphics.CopyFromScreen(rect.Left, rect.Top, 0, 0, before.Size); }
            RedrawWindow(window, IntPtr.Zero, IntPtr.Zero, 0x101);
            DwmFlush();
            using (var graphics = Graphics.FromImage(after)) { graphics.CopyFromScreen(rect.Left, rect.Top, 0, 0, after.Size); }
            before.Save(prefix + "-before.png", ImageFormat.Png); after.Save(prefix + "-after.png", ImageFormat.Png);
            int changed = 0;
            for (int x = 0; x < width; x++) for (int y = 0; y < height; y++) if (before.GetPixel(x, y) != after.GetPixel(x, y)) changed++;
            return changed;
        }
    }
}
'@
function Assert-Ui([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Wait-Ui([scriptblock]$Condition, [string]$Message) {
    $timeout = [Diagnostics.Stopwatch]::StartNew()
    while ($timeout.Elapsed.TotalSeconds -lt 7) { if (& $Condition) { return }; Start-Sleep -Milliseconds 60 }
    throw $Message
}
function Read-Menu([IntPtr]$TestHost, [IntPtr]$Target, [bool]$Tray, [bool]$Posted = $false) {
    if ($Tray -and $Posted) { [LeafShieldUiTest]::PostMessageW($TestHost, 0x8006, [IntPtr]1, [IntPtr]0x205) > $null }
    elseif ($Tray) { [LeafShieldUiTest]::SendMessageW($TestHost, 0x8006, [IntPtr]1, [IntPtr]0x205) > $null }
    else { [LeafShieldUiTest]::SendMessageW($Target, 0x7b, $Target, [IntPtr]::new(-1)) > $null }
    Wait-Ui { [LeafShieldUiTest]::GetPropW($TestHost, 'LeafShield.ActiveMenu') -ne [IntPtr]::Zero } 'Right-click menu did not open'
    $menu = [LeafShieldUiTest]::GetPropW($TestHost, 'LeafShield.ActiveMenu')
    $items = [LeafShieldUiTest]::Menu($menu)
    [LeafShieldUiTest]::PostMessageW($TestHost, 0x1f, [IntPtr]::Zero, [IntPtr]::Zero) > $null
    Wait-Ui { [LeafShieldUiTest]::GetPropW($TestHost, 'LeafShield.ActiveMenu') -eq [IntPtr]::Zero } 'Menu did not close'
    return $items
}
function Set-Ui-Language([IntPtr]$MainWindow, [int]$Index) {
    [LeafShieldUiTest]::SendMessageW([LeafShieldUiTest]::GetDlgItem($MainWindow,104), 0xf5, [IntPtr]::Zero, [IntPtr]::Zero) > $null
    Start-Sleep -Milliseconds 120
    $combo = [LeafShieldUiTest]::GetDlgItem($MainWindow,1607)
    [LeafShieldUiTest]::SendMessageW($combo,0x14e,[IntPtr]$Index,[IntPtr]::Zero) > $null
    [LeafShieldUiTest]::PostMessageW($MainWindow,0x111,[IntPtr](65536 + 1607),$combo) > $null
    Wait-Ui { -not [LeafShieldUiTest]::IsWindow($combo) } 'Language switch did not rebuild the settings page'
}
$oldDpi = [LeafShieldUiTest]::SetThreadDpiAwarenessContext([IntPtr]::new(-4))
$ownedProcess = $null
$checks = [Collections.Generic.List[object]]::new()
try {
    $ownedProcess = Start-Process -FilePath $Executable -ArgumentList @('--ui-test-profile', ('"' + $profileDirectory + '"')) -WindowStyle Hidden -PassThru
    Wait-Ui { [LeafShieldUiTest]::Find($ownedProcess.Id, 2) -ne [IntPtr]::Zero } 'Native main window did not start'
    $main = [LeafShieldUiTest]::Find($ownedProcess.Id, 2)
    $testHost = [LeafShieldUiTest]::Find($ownedProcess.Id, 1)
    $widget = [LeafShieldUiTest]::Find($ownedProcess.Id, 3)
    [LeafShieldUiTest]::ShowWindow($main, 4) > $null
    [LeafShieldUiTest]::SetForegroundWindow($main) > $null
    Assert-Ui ([LeafShieldUiTest]::Text($main) -eq 'LeafShield') 'Brand is missing from the window title'
    $checks.Add(@{name='brand-in-title-bar';passed=$true})
    foreach ($sliderId in 1001,1002) {
        $slider = [LeafShieldUiTest]::GetDlgItem($main, $sliderId)
        $bounds = [LeafShieldUiTest+Rect]::new(); [LeafShieldUiTest]::GetClientRect($slider, [ref]$bounds) > $null
        $width = $bounds.Right; $middleY = [int]($bounds.Bottom / 2)
        foreach ($fraction in 0.25,0.5,0.75) {
            $targetX = [int][Math]::Round($width * $fraction)
            [LeafShieldUiTest]::Click($slider, $targetX, $middleY)
            Start-Sleep -Milliseconds 120
            $position = [LeafShieldUiTest]::SendMessageW($slider, 0x400, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
            $pixels = [LeafShieldUiTest]::Capture($slider, (Join-Path $testDirectory ('slider-' + $sliderId + '-' + $fraction + '.png')))
            Assert-Ui ($pixels[2] -gt $width / 2) 'Slider rail is missing'
            Assert-Ui ([Math]::Abs($pixels[4] - $targetX) -le 3) 'Slider thumb is not aligned to the click'
            $minimum = if ($sliderId -eq 1001) { 0 } else { 20 }
            $maximum = if ($sliderId -eq 1001) { 5000 } else { 100 }
            Assert-Ui ([Math]::Abs($position - ($minimum + ($maximum - $minimum) * $fraction)) -lt ($maximum - $minimum) * 0.04) 'Slider value is outside the expected click range'
            $checks.Add(@{name='slider-click-and-visible-rail';slider=$sliderId;fraction=$fraction;position=$position;thumbPixel=$pixels[4];clickPixel=$targetX;accentPixels=$pixels[2];passed=$true})
        }
        [LeafShieldUiTest]::Drag($slider, $pixels[4], [int]($width * 0.5), $middleY)
        Start-Sleep -Milliseconds 120
        $dragged = [LeafShieldUiTest]::SendMessageW($slider, 0x400, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
        Assert-Ui ([Math]::Abs($dragged - ($minimum + ($maximum - $minimum) * 0.5)) -lt ($maximum - $minimum) * 0.05) 'Native slider drag regressed'
        $checks.Add(@{name='slider-drag';slider=$sliderId;position=$dragged;passed=$true})
        foreach ($fraction in 0.2,0.8,0.35,0.7) {
            [LeafShieldUiTest]::Click($slider, [int]($width * $fraction), $middleY)
            Start-Sleep -Milliseconds 100
        }
        $changed = [LeafShieldUiTest]::VisibleRedrawDifference($slider, (Join-Path $testDirectory ('slider-trail-' + $sliderId)))
        Assert-Ui ($changed -eq 0) ('Slider leaves stale pixels before a full redraw: ' + $changed)
        $checks.Add(@{name='slider-no-stale-pixels';slider=$sliderId;changedPixels=$changed;passed=$true})
    }
    [LeafShieldUiTest]::Capture($main, (Join-Path $testDirectory 'display.png')) > $null
    $trayItems = @(Read-Menu $testHost $testHost $true)
    $widgetItems = @(Read-Menu $testHost $widget $false)
    Assert-Ui ($trayItems.Count -eq 6) 'Tray menu should contain six actions'
    Assert-Ui (($trayItems -join '|') -eq ($widgetItems -join '|')) 'Tray and floating timer menus differ'
    $checks.Add(@{name='tray-and-widget-menus-match';items=$trayItems;passed=$true})
    $postedItems = @(Read-Menu $testHost $testHost $true $true)
    Assert-Ui (($trayItems -join '|') -eq ($postedItems -join '|')) 'Posted and sent shell callbacks display different menus'
    $checks.Add(@{name='posted-and-sent-tray-callbacks';passed=$true})
    [LeafShieldUiTest]::SendMessageW($testHost, 0x111, [IntPtr]11, [IntPtr]::Zero) > $null
    Start-Sleep -Milliseconds 150
    $pausedItems = @(Read-Menu $testHost $testHost $true)
    Assert-Ui ($pausedItems[1] -ne $trayItems[1]) 'Tray menu did not change to Resume after pausing'
    [LeafShieldUiTest]::SendMessageW($testHost, 0x111, [IntPtr]11, [IntPtr]::Zero) > $null
    $checks.Add(@{name='tray-pause-resume';passed=$true})
    foreach ($page in 0..5) {
        $button = [LeafShieldUiTest]::GetDlgItem($main, 100 + $page)
        [LeafShieldUiTest]::SendMessageW($button, 0xf5, [IntPtr]::Zero, [IntPtr]::Zero) > $null
        Start-Sleep -Milliseconds 120
        [LeafShieldUiTest]::Capture($main, (Join-Path $testDirectory ('page-' + $page + '.png'))) > $null
    }
    $checks.Add(@{name='all-six-pages-render';passed=$true})
    foreach ($languageIndex in 0,1) {
        if ($languageIndex -eq 1) { Set-Ui-Language $main 1 }
        [LeafShieldUiTest]::SendMessageW([LeafShieldUiTest]::GetDlgItem($main,102),0xf5,[IntPtr]::Zero,[IntPtr]::Zero) > $null
        Start-Sleep -Milliseconds 120
        $help = [LeafShieldUiTest]::GetDlgItem($main,1414)
        $description = [LeafShieldUiTest]::Text($help)
        Assert-Ui ([LeafShieldUiTest]::IsWindowVisible($help)) 'Emergency unlock explanation is missing'
        Assert-Ui ($description.Contains('50') -and $description.Contains('20') -and $description.Contains('2')) 'Emergency explanation omits the key count or time limits'
        Assert-Ui ([LeafShieldUiTest]::TextFits($help)) 'Emergency explanation is clipped'
        $helpBounds = [LeafShieldUiTest+Rect]::new(); [LeafShieldUiTest]::GetWindowRect($help,[ref]$helpBounds) > $null
        $nextBounds = [LeafShieldUiTest+Rect]::new(); [LeafShieldUiTest]::GetWindowRect([LeafShieldUiTest]::GetDlgItem($main,1412),[ref]$nextBounds) > $null
        Assert-Ui ($helpBounds.Bottom -le $nextBounds.Top) 'Emergency explanation overlaps wallpaper settings'
        if ($languageIndex -eq 1) { Assert-Ui ($description.Contains('logged') -and $description.Contains('Holding')) 'English emergency explanation is incomplete' }
        [LeafShieldUiTest]::Capture($main,(Join-Path $testDirectory ('emergency-help-' + $languageIndex + '.png'))) > $null
        $checks.Add(@{name='emergency-unlock-description';language=$languageIndex;text=$description;fits=$true;passed=$true})
    }
    Set-Ui-Language $main 0
    [LeafShieldUiTest]::SendMessageW($main, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) > $null
    Wait-Ui { -not [LeafShieldUiTest]::IsWindow($main) } 'Closing settings did not destroy the window'
    $backgroundMenu = @(Read-Menu $testHost $testHost $true)
    Assert-Ui (($backgroundMenu -join '|') -eq ($trayItems -join '|')) 'Tray menu is unavailable after closing settings'
    [LeafShieldUiTest]::SendMessageW($testHost, 0x111, [IntPtr]10, [IntPtr]::Zero) > $null
    Wait-Ui { [LeafShieldUiTest]::Find($ownedProcess.Id, 2) -ne [IntPtr]::Zero } 'Tray Show action did not reopen settings'
    $checks.Add(@{name='tray-menu-with-settings-closed';passed=$true})
    $report = @{ok=$true;executable=$Executable;sha256=(Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash;checks=$checks.ToArray();profile=$profileDirectory;scope='Real native windows, message-driven clicks/drags, pixel checks, and tray callbacks. Test profile disables global keyboard hooks, display adjustment, and production startup writes.'}
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $testDirectory 'report.json') -Encoding UTF8
    $report | ConvertTo-Json -Depth 8
} catch {
    @{ok=$false;error=$_.Exception.Message;checks=$checks.ToArray()} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $testDirectory 'report.json') -Encoding UTF8
    throw
} finally {
    if ($ownedProcess -and -not $ownedProcess.HasExited) {
        $testHost = [LeafShieldUiTest]::Find($ownedProcess.Id, 1)
        if ($testHost -ne [IntPtr]::Zero) { [LeafShieldUiTest]::PostMessageW($testHost, 0x1f, [IntPtr]::Zero, [IntPtr]::Zero) > $null; [LeafShieldUiTest]::PostMessageW($testHost, 0x111, [IntPtr]15, [IntPtr]::Zero) > $null }
        if (-not $ownedProcess.WaitForExit(3000)) { Stop-Process -Id $ownedProcess.Id }
    }
    [LeafShieldUiTest]::SetThreadDpiAwarenessContext($oldDpi) > $null
}
