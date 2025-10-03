# Windows Installation Guide

This guide explains how to use the Windows installation scripts for FerrisUnzip.

## Overview

FerrisUnzip provides two PowerShell scripts for Windows users:
- `install_windows.ps1` - Installs FerrisUnzip with system integration
- `uninstall_windows.ps1` - Removes FerrisUnzip system integration

## Features

### 1. PATH Integration
Adds the FerrisUnzip executable directory to your Windows PATH, allowing you to run `Archiver` from any directory in the command line or PowerShell.

**User-level PATH**: Works without administrator privileges (affects only your user account)
**System-level PATH**: Requires administrator privileges (affects all users on the system)

### 2. Windows Explorer Context Menu
Adds a "Extract with FerrisUnzip" option to the right-click context menu for all files in Windows Explorer.

**Note**: This feature requires administrator privileges to modify the Windows Registry.

## Installation Instructions

### Basic Installation (User-level)

1. Build FerrisUnzip:
   ```powershell
   cargo build --release
   ```

2. Run the installation script:
   ```powershell
   .\install_windows.ps1
   ```

3. Follow the prompts to select your installation options:
   - Option 1: Add to PATH only
   - Option 2: Add Windows Explorer context menu only
   - Option 3: Both PATH and context menu
   - Option 4: Exit without installing

### Administrator Installation (System-level)

For system-wide installation or to add context menu integration:

1. Right-click on PowerShell and select "Run as Administrator"

2. Navigate to the FerrisUnzip directory:
   ```powershell
   cd C:\path\to\FerrisUnzip
   ```

3. Run the installation script:
   ```powershell
   .\install_windows.ps1
   ```

## Uninstallation Instructions

To remove FerrisUnzip integration:

1. Run the uninstallation script:
   ```powershell
   .\uninstall_windows.ps1
   ```

2. Follow the prompts to select what you want to remove:
   - Option 1: Remove from PATH only
   - Option 2: Remove Windows Explorer context menu only
   - Option 3: Remove both PATH and context menu
   - Option 4: Exit without uninstalling

**Note**: The uninstallation script does not delete the executable files. You can manually delete the `target/release` directory if you want to completely remove FerrisUnzip.

## Troubleshooting

### Script execution is disabled
If you encounter an error about script execution being disabled, you may need to change your PowerShell execution policy:

```powershell
# Temporary change (for current session only)
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass

# Or allow script execution for the current user
Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned
```

Then run the installation script again.

### Context menu not appearing
- Make sure you ran the script as Administrator
- Try restarting Windows Explorer (or reboot your computer)
- Check if the registry entry was created at: `HKEY_CLASSES_ROOT\*\shell\FerrisUnzip`

### PATH changes not taking effect
- Close and reopen your command prompt or PowerShell window
- In some cases, you may need to log out and log back in, or restart your computer

### Permission errors
- For system-wide installation or context menu integration, you must run PowerShell as Administrator
- User-level PATH installation works without administrator privileges

## Manual Installation

If you prefer to install manually:

### Adding to PATH manually
1. Open "System Properties" > "Advanced" > "Environment Variables"
2. Under "User variables" or "System variables", edit the "Path" variable
3. Add the full path to `target\release` directory
4. Click OK to save

### Adding context menu manually
1. Open Registry Editor (regedit.exe) as Administrator
2. Navigate to: `HKEY_CLASSES_ROOT\*\shell`
3. Create a new key named "FerrisUnzip"
4. Set the default value to "Extract with FerrisUnzip"
5. Create a new string value named "Icon" and set it to the full path of Archiver.exe
6. Create a new subkey under FerrisUnzip named "command"
7. Set the default value to: `"C:\full\path\to\Archiver.exe" "%1"` (replace with actual path)

## Usage After Installation

### Command Line
After adding to PATH, you can use FerrisUnzip from anywhere:
```powershell
Archiver myarchive.zip
Archiver myarchive.7z -p mypassword
```

### Windows Explorer Context Menu
1. Right-click any archive file (ZIP, 7Z, TAR, RAR, etc.)
2. Select "Extract with FerrisUnzip"
3. Follow the prompts in the console window that appears

## Notes

- The executable name is `Archiver.exe` (as specified in Cargo.toml), not `FerrisUnzip.exe`
- Context menu integration works with all file types, but FerrisUnzip will only successfully extract supported archive formats
- Supported formats: ZIP, 7Z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, and RAR
