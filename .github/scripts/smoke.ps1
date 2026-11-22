# Essai de bout en bout d'un binaire sous Windows : aide, hooks, démon, un événement, appairage.
# Usage : smoke.ps1 <chemin du binaire>. Pendant de smoke.sh : n'utilise que le port et les dossiers
# d'essai, jamais ceux de la machine.
param([Parameter(Mandatory = $true)][string]$Bin)

$ErrorActionPreference = 'Stop'
$Bin = (Resolve-Path $Bin).Path
$port = 47796
$work = Join-Path ([IO.Path]::GetTempPath()) ("tallylight-smoke-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path "$work\home", "$work\local" | Out-Null

# Dossiers par défaut de Windows, déplacés : c'est ce chemin-là qu'on veut voir marcher
$env:USERPROFILE = "$work\home"
$env:LOCALAPPDATA = "$work\local"
$env:TALLYLIGHT_PORT = "$port"
Remove-Item Env:XDG_DATA_HOME, Env:CLAUDE_CONFIG_DIR -ErrorAction SilentlyContinue
[Console]::OutputEncoding = [Text.Encoding]::UTF8

# Lance le binaire et s'arrête à la première commande en échec
function Tally {
    & $Bin @args
    if ($LASTEXITCODE -ne 0) { throw "tallylight $args failed ($LASTEXITCODE)" }
}

function Expect([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}

Tally --version
Tally --help | Out-Null
$hooks = Tally hooks --port $port | Out-String
Expect ($hooks.Contains("127.0.0.1:$port/hooks/claude")) 'the hooks do not point at the test port'
Tally install --no-service
$settings = "$work\home\.claude\settings.json"
Expect (Test-Path $settings) "no settings file at $settings"
Expect ((Get-Content $settings -Raw).Contains('hooks/claude')) 'the hooks are not in the settings'

$daemon = Start-Process -FilePath $Bin -ArgumentList 'daemon', '--port', "$port" -PassThru -NoNewWindow `
    -RedirectStandardOutput "$work\daemon.log" -RedirectStandardError "$work\daemon.err"
try {
    $tries = 0
    while ($true) {
        try { Invoke-RestMethod "http://127.0.0.1:$port/sessions" | Out-Null; break } catch {}
        $tries++
        Expect ($tries -lt 50) 'the daemon did not open its port'
        Start-Sleep -Milliseconds 200
    }

    $hook = '{"hook_event_name":"PreToolUse","session_id":"smoke","cwd":"C:\\Users\\me\\demo","tool_name":"Edit","tool_input":{"file_path":"C:\\Users\\me\\demo\\src\\main.rs"}}'
    Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$port/hooks/claude" -ContentType 'application/json' -Body $hook | Out-Null
    $state = (Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$port/sessions").Content
    Write-Output $state
    Expect ($state.Contains('"working"')) 'the session is not working'
    # Seul le nom du fichier sort de la machine, pas son dossier
    Expect ($state.Contains('"detail":"main.rs"')) 'the detail is not the file name'

    # Données dans %LOCALAPPDATA%\tallylight
    Expect (Test-Path "$work\local\tallylight\key") 'no identity in %LOCALAPPDATA%\tallylight'

    # L'appairage demande le réseau : c'est la preuve que la machine rejoint un relais
    $pair = Tally pair | Out-String
    Write-Output $pair
    Expect ($pair.Contains('#')) 'no pairing string'
    Tally devices
    Tally name 'Smoke test'
    $state = (Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$port/sessions").Content
    Expect ($state.Contains('"machine":"Smoke test"')) 'the daemon did not take the new name'
    Write-Output 'smoke test passed'
}
finally {
    Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
    Get-Content "$work\daemon.log", "$work\daemon.err" -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
