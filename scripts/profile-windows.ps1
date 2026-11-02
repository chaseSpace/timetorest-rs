param([ValidateRange(5,30)][int]$Seconds = 10)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'This sampler is for Windows.' }
$projectRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $projectRoot 'artifacts\leafshield.exe'
if (-not (Test-Path -LiteralPath $executable)) { throw 'Run npm run build first.' }
if (Get-Process -Name leafshield -ErrorAction SilentlyContinue) { throw 'Close the existing LeafShield instance before sampling.' }
$ownedProcess = $null
try {
    $ownedProcess = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 10
    if ($ownedProcess.HasExited) { throw ('Application exited during startup: ' + $ownedProcess.ExitCode) }
    function Get-Family {
        $processes = @(Get-CimInstance Win32_Process)
        $ids = [Collections.Generic.HashSet[int]]::new()
        [void]$ids.Add($ownedProcess.Id)
        do { $changed = $false; foreach ($process in $processes) { if ($ids.Contains([int]$process.ParentProcessId) -and $ids.Add([int]$process.ProcessId)) { $changed = $true } } } while ($changed)
        @($processes | Where-Object { $ids.Contains([int]$_.ProcessId) })
    }
    $initial = @{}
    foreach ($member in @(Get-Family)) { $p = Get-Process -Id $member.ProcessId -ErrorAction SilentlyContinue; if ($p) { $initial[$p.Id] = $p.TotalProcessorTime.TotalSeconds } }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds $Seconds
    $clock.Stop()
    $family = @(Get-Family)
    $cpu = 0.0
    $hostCpuSeconds = 0.0
    $childMemory = 0L
    $childPrivateMemory = 0L
    foreach ($member in $family) {
        $p = Get-Process -Id $member.ProcessId -ErrorAction SilentlyContinue
        if (-not $p) { continue }
        $baseline = if ($initial.ContainsKey($p.Id)) { $initial[$p.Id] } else { 0.0 }
        $delta = [Math]::Max(0, $p.TotalProcessorTime.TotalSeconds - $baseline)
        $cpu += $delta
        if ($p.Id -eq $ownedProcess.Id) { $hostCpuSeconds = $delta }
        else { $childMemory += $p.WorkingSet64; $childPrivateMemory += $p.PrivateMemorySize64 }
    }
    $ownedProcess.Refresh()
    $processors = [Environment]::ProcessorCount
    $hostCpu = $hostCpuSeconds / $clock.Elapsed.TotalSeconds / $processors * 100
    $ids = @($family | Select-Object -ExpandProperty ProcessId)
    $listeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.OwningProcess -in $ids })
    $result = [ordered]@{
        sampledAt = (Get-Date -Format o)
        mode = 'normal-working-state'
        seconds = [Math]::Round($clock.Elapsed.TotalSeconds,2)
        logicalProcessors = $processors
        rustHostWorkingSetMiB = [Math]::Round($ownedProcess.WorkingSet64 / 1MB,2)
        rustHostPrivateMiB = [Math]::Round($ownedProcess.PrivateMemorySize64 / 1MB,2)
        rustHostCpuPercent = [Math]::Round($hostCpu,3)
        webviewChildrenWorkingSetMiB = [Math]::Round($childMemory / 1MB,2)
        webviewChildrenPrivateMiB = [Math]::Round($childPrivateMemory / 1MB,2)
        familyCpuPercent = [Math]::Round($cpu / $clock.Elapsed.TotalSeconds / $processors * 100,3)
        familyProcessCount = $family.Count
        listeningTcpPorts = @($listeners | Select-Object -ExpandProperty LocalPort)
        scope = 'Working state only; CPU normalized by logical processor count; working sets include shared pages.'
    }
    $report = Join-Path $projectRoot 'artifacts\performance.json'
    $result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $report -Encoding UTF8
    $result | ConvertTo-Json -Depth 4
} finally {
    if ($ownedProcess -and -not $ownedProcess.HasExited) { Stop-Process -Id $ownedProcess.Id -Force }
}
