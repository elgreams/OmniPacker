# OmniPacker Windows x64 Build Script

$ErrorActionPreference = "Stop"

Write-Host "========================================"
Write-Host "Building OmniPacker for Windows x64"
Write-Host "Target: x86_64-pc-windows-msvc"
Write-Host "========================================"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$binariesDir = Join-Path $projectRoot "src-tauri\binaries"
$configFile = Join-Path $projectRoot "src-tauri\tauri.conf.json"
$tempBackup = New-Item -ItemType Directory -Path ([System.IO.Path]::GetTempPath()) -Name "omnipacker-binaries-backup-$(Get-Random)"

Write-Host "Creating backup of binaries in $tempBackup"
Copy-Item -Path $binariesDir -Destination $tempBackup -Recurse

Write-Host "Creating backup of tauri.conf.json"
Copy-Item -Path $configFile -Destination "$configFile.backup"

function Restore-Files {
    Write-Host "Restoring original files..."
    Remove-Item -Path $binariesDir -Recurse -Force -ErrorAction SilentlyContinue
    Move-Item -Path "$tempBackup\binaries" -Destination (Split-Path $binariesDir) -Force
    Remove-Item -Path $tempBackup -Recurse -Force -ErrorAction SilentlyContinue

    if (Test-Path "$configFile.backup") {
        Move-Item -Path "$configFile.backup" -Destination $configFile -Force
    }
}

try {
    Write-Host "Modifying tauri.conf.json to only include win-x64 resources..."
    $config = Get-Content $configFile -Raw | ConvertFrom-Json
    $config.bundle.resources = @("binaries/win-x64/*")
    $config | ConvertTo-Json -Depth 10 | Set-Content $configFile

    Write-Host "Removing non-target platform binaries..."
    Get-ChildItem -Path $binariesDir -Directory | Where-Object { $_.Name -ne "win-x64" } | ForEach-Object {
        Write-Host "  Removing $($_.Name)"
        Remove-Item -Path $_.FullName -Recurse -Force
    }

    Write-Host "`nRemaining binaries:"
    Get-ChildItem -Path $binariesDir

    Set-Location $projectRoot
    Write-Host "`nStarting Tauri build..."
    npm run tauri build -- --target x86_64-pc-windows-msvc

    Write-Host "`n========================================"
    Write-Host "Build complete!"
    Write-Host "Artifacts:"
    Write-Host "  MSI: src-tauri\target\x86_64-pc-windows-msvc\release\bundle\msi\"
    Write-Host "  NSIS: src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\"
    Write-Host "========================================"
}
finally {
    Restore-Files
}
