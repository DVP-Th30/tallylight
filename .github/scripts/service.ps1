# Essai du service sous Windows : `install` crée la tâche planifiée et lance le démon, une seconde
# installation le relance, `uninstall` retire tout. Usage : service.ps1 <chemin du binaire>.
# Réservé à une machine d'essai : utilise le port par défaut et les réglages de l'utilisateur.
param([Parameter(Mandatory = $true)][string]$Bin)

$ErrorActionPreference = 'Stop'
$Bin = (Resolve-Path $Bin).Path
$port = 47700
$log = "$env:LOCALAPPDATA\tallylight\daemon.log"
[Console]::OutputEncoding = [Text.Encoding]::UTF8

function Tally {
    & $Bin @args
    if ($LASTEXITCODE -ne 0) { throw "tallylight $args failed ($LASTEXITCODE)" }
}

function Expect([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}

function TaskExists {
    cmd /c "schtasks /Query /TN tallylight >nul 2>&1"
    return $LASTEXITCODE -eq 0
}

function DaemonAnswers {
    try { Invoke-RestMethod "http://127.0.0.1:$port/sessions" | Out-Null; return $true } catch { return $false }
}

try {
    $installed = Tally install | Out-String
    Write-Output $installed
    Expect (TaskExists) 'the scheduled task was not created'
    Expect ($installed.Contains('service: started')) 'install did not see the daemon start'
    Expect (DaemonAnswers) 'the daemon started by the task does not answer'
    schtasks /Query /TN tallylight /V /FO LIST
    $first = (Get-Process tallylight | Where-Object { $_.Id -ne $PID }).Id
    Write-Output "daemon process: $first"

    Tally pair
    Expect (Test-Path $log) "no log at $log"
    Expect ((Get-Content $log -Raw).Contains('listening for hooks')) 'the log does not hold the daemon output'

    # Réinstallation avec un relais : le démon est relancé, le réglage est gardé ensuite
    $again = Tally install --relay 'https://relay.example.com' | Out-String
    Write-Output $again
    Expect ($again.Contains('relay: https://relay.example.com/')) 'the relay was not taken'
    Expect (DaemonAnswers) 'the daemon does not answer after a second install'
    $second = (Get-Process tallylight | Where-Object { $_.Id -ne $PID }).Id
    Expect ("$second" -ne "$first") 'the daemon was not restarted'
    $kept = Tally install | Out-String
    Expect ($kept.Contains('relay: https://relay.example.com/')) 'the relay was lost on reinstall'

    Tally uninstall
    Expect (-not (TaskExists)) 'the scheduled task is still there'
    Start-Sleep -Seconds 1
    Expect (-not (DaemonAnswers)) 'the daemon still answers after uninstall'
    Write-Output 'service test passed'
}
finally {
    Get-Content $log -ErrorAction SilentlyContinue
    cmd /c "schtasks /End /TN tallylight >nul 2>&1"
    cmd /c "schtasks /Delete /TN tallylight /F >nul 2>&1"
    $global:LASTEXITCODE = 0
}
