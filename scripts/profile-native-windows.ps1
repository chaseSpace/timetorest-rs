param(
    [ValidateSet('tray','floating','settings','rest')][string]$Mode = 'floating',
    [ValidateRange(3,30)][int]$Seconds = 15,
    [int]$ProcessId = 0
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$expectedDirectory = [IO.Path]::GetFullPath((Join-Path $projectRoot 'artifacts\native-dev'))
$candidates = @(Get-CimInstance Win32_Process | Where-Object {
    $_.Name -like 'leafshield-native*.exe' -and $_.ExecutablePath -and
    [IO.Path]::GetDirectoryName($_.ExecutablePath) -eq $expectedDirectory -and
    ($ProcessId -eq 0 -or $_.ProcessId -eq $ProcessId)
})
if ($candidates.Count -ne 1) { throw 'Start exactly one native development EXE first, or specify -ProcessId.' }
$nativeId = [int]$candidates[0].ProcessId
$initial = Get-Process -Id $nativeId
$started = $initial.StartTime
$initialCpu = $initial.TotalProcessorTime.TotalSeconds
$clock = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds $Seconds
$nativeProcess = Get-Process -Id $nativeId
if ($nativeProcess.StartTime -ne $started) { throw 'The measured process restarted.' }
$allProcesses = @(Get-CimInstance Win32_Process)
$ownedIds = [Collections.Generic.HashSet[int]]::new()
[void]$ownedIds.Add($nativeId)
do {
    $added = $false
    foreach ($candidate in $allProcesses) {
        if ($ownedIds.Contains([int]$candidate.ParentProcessId) -and $ownedIds.Add([int]$candidate.ProcessId)) { $added = $true }
    }
} while ($added)
$family = @($allProcesses | Where-Object { $ownedIds.Contains([int]$_.ProcessId) })
$result = [ordered]@{
    sampledAt = (Get-Date -Format o)
    mode = $Mode
    executable = $candidates[0].ExecutablePath
    processId = $nativeId
    seconds = [Math]::Round($clock.Elapsed.TotalSeconds, 2)
    privateCommitMiB = [Math]::Round($nativeProcess.PrivateMemorySize64 / 1MB, 2)
    workingSetMiB = [Math]::Round($nativeProcess.WorkingSet64 / 1MB, 2)
    cpuPercent = [Math]::Round(($nativeProcess.TotalProcessorTime.TotalSeconds - $initialCpu) / $clock.Elapsed.TotalSeconds / [Environment]::ProcessorCount * 100, 3)
    handleCount = $nativeProcess.HandleCount
    familyProcessCount = $family.Count
    webviewProcessCount = @($family | Where-Object { $_.Name -eq 'msedgewebview2.exe' }).Count
    scope = 'Read-only sample of an already running app. Mode is selected manually. Private commit and working set use different accounting.'
}
$report = Join-Path $expectedDirectory ('performance-' + $Mode + '.json')
$result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $report -Encoding UTF8
$result | ConvertTo-Json -Depth 4
