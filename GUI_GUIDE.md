# FerrisUnzip GUI Guide

## Overview

FerrisUnzip features a modern, user-friendly graphical interface built with Slint, providing a cross-platform experience on Windows, macOS, and Linux. The UI is defined declaratively using the Slint language, ensuring a native look and feel on all platforms.

## GUI Layout

```
┌─────────────────────────────────────────────────────────┐
│                  FerrisUnzip - Archive Extractor        │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Archive file:    [Browse...]                           │
│  Path: [_________________________________]              │
│                                                          │
│  Extract to:      [Browse...]                           │
│  Path: [_________________________________]              │
│                                                          │
│  Password (optional): [*********]                       │
│  (Leave blank for non-encrypted archives)               │
│                                                          │
│  [Extract Archive]  ⟳ (spinner when extracting)        │
│                                                          │
├─────────────────────────────────────────────────────────┤
│  Status: Select an archive file to extract              │
│                                                          │
│  Supported formats: ZIP, 7Z, TAR, TAR.GZ, TAR.BZ2,     │
│  TAR.XZ, GZ, BZ2, XZ, RAR                               │
│  Version 1.0 - Cross-platform archive extractor        │
└─────────────────────────────────────────────────────────┘
```

## Features

### 1. Archive File Selection
- **Browse button**: Opens a native file picker dialog
- **Path field**: Displays selected archive path, can also be manually edited
- **Supported formats**: Filters for common archive types

### 2. Extraction Destination
- **Browse button**: Opens a native folder picker dialog
- **Path field**: Shows extraction destination, editable
- **Auto-suggestion**: Automatically suggests extraction path based on archive location

### 3. Password Protection
- **Password field**: Securely enters passwords (masked with asterisks)
- **Optional**: Only required for encrypted 7Z archives
- **Hint text**: Provides guidance when field is empty

### 4. Extract Button
- **Enabled/Disabled**: Only active when archive and destination are selected
- **Progress indicator**: Shows a spinner during extraction
- **Non-blocking**: Extraction runs in background thread, UI remains responsive

### 5. Status Messages
- **Color-coded feedback**:
  - Green (✓): Successful extraction
  - Red (✗): Extraction errors
  - Blue: In progress
  - Gray: Informational messages

## Usage Flow

1. **Launch the application** without command-line arguments
2. **Click "Browse..."** next to "Archive file" to select an archive
3. **Verify or change** the extraction destination (auto-populated)
4. **Enter password** if the archive is encrypted (optional)
5. **Click "Extract Archive"** to begin extraction
6. **Monitor status** at the bottom of the window
7. **Files extracted** to the specified directory when complete

## Advantages of GUI Mode

- **Visual feedback**: See extraction progress and status in real-time
- **Easy file selection**: Native file dialogs integrated with your OS
- **No command-line knowledge required**: Point and click interface
- **Error handling**: Clear error messages displayed in the interface
- **Multi-tasking**: Window can be minimized or moved while extracting
