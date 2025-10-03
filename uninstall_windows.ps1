# FerrisUnzip Windows Uninstallation Script
# This script removes FerrisUnzip installation by:
# 1. Removing the executable from the system PATH (if present)
# 2. Removing the context menu entry from Windows Explorer (if present)

# Check if running as Administrator
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin) {
    Write-Host "Warning: This script is not running as Administrator." -ForegroundColor Yellow
    Write-Host "Some features (like removing from system PATH or context menu) may require administrator privileges." -ForegroundColor Yellow
    Write-Host "You can still proceed with user-level uninstallation." -ForegroundColor Yellow
    Write-Host ""
}

# Get the script directory
$scriptPath = Split-Path -Parent $MyInvocation.MyCommand.Path
$targetPath = Join-Path $scriptPath "target\release"

Write-Host "FerrisUnzip Windows Uninstallation Script" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""

# Ask user what they want to uninstall
Write-Host "What would you like to uninstall?" -ForegroundColor Yellow
Write-Host "1. Remove from PATH only"
Write-Host "2. Remove Windows Explorer context menu only"
Write-Host "3. Remove both PATH and context menu"
Write-Host "4. Exit without uninstalling"
Write-Host ""

$choice = Read-Host "Enter your choice (1-4)"

$removeFromPath = $false
$removeContextMenu = $false

switch ($choice) {
    "1" { $removeFromPath = $true }
    "2" { $removeContextMenu = $true }
    "3" { 
        $removeFromPath = $true
        $removeContextMenu = $true
    }
    "4" { 
        Write-Host "Uninstallation cancelled." -ForegroundColor Yellow
        exit 0
    }
    default {
        Write-Host "Invalid choice. Uninstallation cancelled." -ForegroundColor Red
        exit 1
    }
}

# Function to remove from PATH
function Remove-FromPath {
    param(
        [string]$directory
    )
    
    $removed = $false
    
    # Try to remove from system PATH
    if ($isAdmin) {
        Write-Host "Removing from system PATH..." -ForegroundColor Cyan
        $currentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")
        
        if ($currentPath -like "*$directory*") {
            $pathArray = $currentPath -split ";" | Where-Object { $_ -ne $directory -and $_ -notlike "*$directory*" }
            $newPath = $pathArray -join ";"
            [Environment]::SetEnvironmentVariable("Path", $newPath, "Machine")
            Write-Host "Successfully removed from system PATH!" -ForegroundColor Green
            $removed = $true
        } else {
            Write-Host "Not found in system PATH." -ForegroundColor Yellow
        }
    }
    
    # Try to remove from user PATH
    Write-Host "Removing from user PATH..." -ForegroundColor Cyan
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    
    if ($currentPath -like "*$directory*") {
        $pathArray = $currentPath -split ";" | Where-Object { $_ -ne $directory -and $_ -notlike "*$directory*" }
        $newPath = $pathArray -join ";"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "Successfully removed from user PATH!" -ForegroundColor Green
        $removed = $true
    } else {
        Write-Host "Not found in user PATH." -ForegroundColor Yellow
    }
    
    if ($removed) {
        Write-Host "You may need to restart your terminal for changes to take effect." -ForegroundColor Yellow
    }
}

# Function to remove context menu
function Remove-ContextMenu {
    Write-Host "Removing Windows Explorer context menu..." -ForegroundColor Cyan
    
    try {
        # Registry path for the context menu
        $regPath = "Registry::HKEY_CLASSES_ROOT\*\shell\FerrisUnzip"
        
        if (Test-Path $regPath) {
            if ($isAdmin) {
                Remove-Item -Path $regPath -Recurse -Force
                Write-Host "Successfully removed context menu entry!" -ForegroundColor Green
            } else {
                Write-Host "Error: Administrator privileges required to remove context menu entries." -ForegroundColor Red
                Write-Host "Please run this script as Administrator and try again." -ForegroundColor Yellow
                return $false
            }
        } else {
            Write-Host "Context menu entry not found." -ForegroundColor Yellow
        }
    } catch {
        Write-Host "Error removing context menu: $_" -ForegroundColor Red
        return $false
    }
    
    return $true
}

# Execute the selected options
if ($removeFromPath) {
    Remove-FromPath -directory $targetPath
    Write-Host ""
}

if ($removeContextMenu) {
    $result = Remove-ContextMenu
    Write-Host ""
}

Write-Host "Uninstallation complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Note: The executable files in target/release are not deleted." -ForegroundColor Cyan
Write-Host "You can manually delete them if you no longer need them." -ForegroundColor Cyan
