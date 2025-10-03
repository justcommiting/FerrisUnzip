# FerrisUnzip Windows Installation Script
# This script installs FerrisUnzip by:
# 1. Adding the executable to the system PATH (optional)
# 2. Adding a context menu entry for Windows Explorer right-click (optional)

# Check if running as Administrator
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin) {
    Write-Host "Warning: This script is not running as Administrator." -ForegroundColor Yellow
    Write-Host "Some features (like adding to system PATH) may require administrator privileges." -ForegroundColor Yellow
    Write-Host "You can still proceed with user-level installation." -ForegroundColor Yellow
    Write-Host ""
}

# Get the script directory (where the executable should be)
$scriptPath = Split-Path -Parent $MyInvocation.MyCommand.Path
$exePath = Join-Path $scriptPath "target\release\Archiver.exe"

# Check if the executable exists
if (-not (Test-Path $exePath)) {
    Write-Host "Error: Archiver.exe not found at $exePath" -ForegroundColor Red
    Write-Host "Please build the project first using: cargo build --release" -ForegroundColor Yellow
    exit 1
}

Write-Host "FerrisUnzip Windows Installation Script" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Found executable at: $exePath" -ForegroundColor Green
Write-Host ""

# Ask user what they want to install
Write-Host "What would you like to install?" -ForegroundColor Yellow
Write-Host "1. Add to PATH only"
Write-Host "2. Add Windows Explorer context menu only"
Write-Host "3. Both PATH and context menu"
Write-Host "4. Exit without installing"
Write-Host ""

$choice = Read-Host "Enter your choice (1-4)"

$addToPath = $false
$addContextMenu = $false

switch ($choice) {
    "1" { $addToPath = $true }
    "2" { $addContextMenu = $true }
    "3" { 
        $addToPath = $true
        $addContextMenu = $true
    }
    "4" { 
        Write-Host "Installation cancelled." -ForegroundColor Yellow
        exit 0
    }
    default {
        Write-Host "Invalid choice. Installation cancelled." -ForegroundColor Red
        exit 1
    }
}

# Function to add to PATH
function Add-ToPath {
    param(
        [string]$directory
    )
    
    $targetPath = Split-Path -Parent $directory
    
    if ($isAdmin) {
        # Add to system PATH
        Write-Host "Adding to system PATH..." -ForegroundColor Cyan
        $currentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")
        
        if ($currentPath -notlike "*$targetPath*") {
            $newPath = "$currentPath;$targetPath"
            [Environment]::SetEnvironmentVariable("Path", $newPath, "Machine")
            Write-Host "Successfully added to system PATH!" -ForegroundColor Green
            Write-Host "You may need to restart your terminal for changes to take effect." -ForegroundColor Yellow
        } else {
            Write-Host "Directory already in system PATH." -ForegroundColor Yellow
        }
    } else {
        # Add to user PATH
        Write-Host "Adding to user PATH..." -ForegroundColor Cyan
        $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
        
        if ($currentPath -notlike "*$targetPath*") {
            if ($currentPath) {
                $newPath = "$currentPath;$targetPath"
            } else {
                $newPath = $targetPath
            }
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
            Write-Host "Successfully added to user PATH!" -ForegroundColor Green
            Write-Host "You may need to restart your terminal for changes to take effect." -ForegroundColor Yellow
        } else {
            Write-Host "Directory already in user PATH." -ForegroundColor Yellow
        }
    }
}

# Function to add context menu
function Add-ContextMenu {
    param(
        [string]$exePath
    )
    
    Write-Host "Adding Windows Explorer context menu..." -ForegroundColor Cyan
    
    try {
        # Registry path for the context menu
        $regPath = "Registry::HKEY_CLASSES_ROOT\*\shell\FerrisUnzip"
        $commandPath = "$regPath\command"
        
        # Check if we can write to HKCR (requires admin)
        if ($isAdmin) {
            # Create the registry keys
            if (-not (Test-Path $regPath)) {
                New-Item -Path $regPath -Force | Out-Null
            }
            
            # Set the menu text
            Set-ItemProperty -Path $regPath -Name "(Default)" -Value "Extract with FerrisUnzip"
            Set-ItemProperty -Path $regPath -Name "Icon" -Value "`"$exePath`"" -Type String
            
            # Create the command subkey
            if (-not (Test-Path $commandPath)) {
                New-Item -Path $commandPath -Force | Out-Null
            }
            
            # Set the command to execute
            Set-ItemProperty -Path $commandPath -Name "(Default)" -Value "`"$exePath`" `"%1`"" -Type String
            
            Write-Host "Successfully added context menu entry!" -ForegroundColor Green
            Write-Host "You can now right-click on archive files and select 'Extract with FerrisUnzip'" -ForegroundColor Green
        } else {
            Write-Host "Error: Administrator privileges required to add context menu entries." -ForegroundColor Red
            Write-Host "Please run this script as Administrator and try again." -ForegroundColor Yellow
            return $false
        }
    } catch {
        Write-Host "Error adding context menu: $_" -ForegroundColor Red
        return $false
    }
    
    return $true
}

# Execute the selected options
if ($addToPath) {
    Add-ToPath -directory $exePath
    Write-Host ""
}

if ($addContextMenu) {
    $result = Add-ContextMenu -exePath $exePath
    Write-Host ""
}

Write-Host "Installation complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Note: The executable name is 'Archiver.exe' (as specified in Cargo.toml)" -ForegroundColor Cyan
Write-Host "You can run it from the command line using: Archiver <archive_file>" -ForegroundColor Cyan
