# screenshot-mcp Windows Installer
# Downloads the latest release from GitHub and adds it to the user PATH

$ErrorActionPreference = "Stop"

$Repo = "username/screenshot-mcp"
$InstallDir = "$env:LOCALAPPDATA\screenshot-mcp"
$BinaryName = "screenshot-mcp.exe"

Write-Host "Installing screenshot-mcp..."

# Create install directory
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

# Fetch latest release URL
$DownloadUrl = "https://github.com/$Repo/releases/latest/download/screenshot-mcp-windows-x86_64.exe"

Write-Host "Downloading from $DownloadUrl..."
Invoke-WebRequest -Uri $DownloadUrl -OutFile "$InstallDir\$BinaryName"

Write-Host "Installed to $InstallDir\$BinaryName"

# Add to PATH
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    Write-Host "Adding to user PATH..."
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Path updated. You may need to restart your terminal."
}

Write-Host "Installation complete!"
