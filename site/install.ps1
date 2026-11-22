# tallylight installer for Windows: downloads the latest release and puts it in
# %LOCALAPPDATA%\tallylight\bin.
#   irm https://dvp-th30.github.io/tallylight/install.ps1 | iex
# Set $env:TALLYLIGHT_VERSION = 'v0.2.0' to pin a version, $env:TALLYLIGHT_BIN_DIR to choose the
# folder, $env:TALLYLIGHT_NO_MODIFY_PATH = '1' to leave your PATH alone.

& {
    $ErrorActionPreference = 'Stop'
    # Windows PowerShell 5.1: the progress bar makes downloads very slow, and TLS 1.2 is not always on
    $ProgressPreference = 'SilentlyContinue'
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo = 'DVP-Th30/tallylight'
    $binDir = if ($env:TALLYLIGHT_BIN_DIR) { $env:TALLYLIGHT_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'tallylight\bin' }

    $processor = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    switch ($processor) {
        'AMD64' { }
        'ARM64' { Write-Host 'Note: no ARM64 build yet, installing the x86_64 one (Windows runs it emulated).' }
        default { throw "tallylight: unsupported processor: $processor" }
    }
    $target = 'x86_64-pc-windows-msvc'
    $archive = "tallylight-$target.zip"

    if ($env:TALLYLIGHT_VERSION) {
        $base = "https://github.com/$repo/releases/download/$($env:TALLYLIGHT_VERSION)"
    }
    else {
        $base = "https://github.com/$repo/releases/latest/download"
    }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("tallylight-install-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Host "Downloading $archive"
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive" -OutFile "$tmp\$archive"
            Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive.sha256" -OutFile "$tmp\$archive.sha256"
        }
        catch {
            throw "tallylight: download failed: $base/$archive ($($_.Exception.Message))"
        }

        $expected = ((Get-Content "$tmp\$archive.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 "$tmp\$archive").Hash.ToLower()
        if ($expected -ne $actual) { throw 'tallylight: checksum mismatch, nothing installed' }

        Expand-Archive -Path "$tmp\$archive" -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Path $binDir -Force | Out-Null
        $exe = Join-Path $binDir 'tallylight.exe'

        # Windows locks a running program: an update stops the background task, then starts it again
        cmd /c "schtasks /Query /TN tallylight >nul 2>&1"
        $service = $LASTEXITCODE -eq 0
        if ($service) { cmd /c "schtasks /End /TN tallylight >nul 2>&1" }
        $copied = $false
        foreach ($attempt in 1..10) {
            try {
                Copy-Item "$tmp\tallylight-$target\tallylight.exe" $exe -Force
                $copied = $true
                break
            }
            catch { Start-Sleep -Milliseconds 500 }
        }
        if ($service) { cmd /c "schtasks /Run /TN tallylight >nul 2>&1" }
        if (-not $copied) {
            throw "tallylight: could not replace $exe. Stop the running tallylight daemon and try again."
        }
    }
    finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }

    $version = & $exe --version
    Write-Host "Installed $version in $binDir"

    # User PATH, read and written without expanding the %VARIABLES% it may hold
    $inPath = ($env:Path -split ';') -contains $binDir
    if (-not $inPath -and -not $env:TALLYLIGHT_NO_MODIFY_PATH) {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        $current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        if (($current -split ';') -notcontains $binDir) {
            $kind = if ($current) { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
            $key.SetValue('Path', (($current.TrimEnd(';') + ';' + $binDir).TrimStart(';')), $kind)
            # Tells running programs (Explorer, so new terminals) that the environment changed
            [Environment]::SetEnvironmentVariable('TALLYLIGHT_PATH_REFRESH', $null, 'User')
            Write-Host "Added $binDir to your PATH (new terminals will see it)."
        }
        $key.Close()
        $env:Path = "$env:Path;$binDir"
    }
    elseif (-not $inPath) {
        Write-Host "Note: $binDir is not in your PATH. Add it, or call $exe directly."
    }

    Write-Host ''
    Write-Host 'Next:'
    Write-Host '  tallylight install    hooks Claude Code and starts the background task'
    Write-Host '  tallylight pair       shows a QR code to pair a screen'
}
