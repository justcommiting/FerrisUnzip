# FerrisUnzip

FerrisUnzip is a cross-platform archive extraction tool written in Rust with both GUI and CLI interfaces. It supports ZIP, 7Z (with password protection), TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, and RAR archives.

## Features

-   **Dual Interface:** Modern graphical user interface (GUI) and command-line interface (CLI).
-   **Cross-platform GUI:** Works on Windows, macOS, and Linux using Slint.
-   **Multi-format support:** Extracts ZIP, 7Z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, and RAR archives.
-   **Password protection:** Supports password-protected 7Z archives.
-   **Interactive extraction:** Easy file and folder selection with native file dialogs.
-   **Visual feedback:** Real-time status updates and progress indicators.
-   **Automatic directory creation:** Creates necessary directories during extraction.
-   **Shell Integration:** One-click installation for context menu integration (right-click "Extract with FerrisUnzip" on Windows/Linux).
-   **Hidden Console:** Clean GUI experience on Windows without console window popup.
-   **Declarative UI:** Modern UI built with Slint for easy customization and maintenance.

## Prerequisites

-   Rust and Cargo installed.

## Installation

1.  Clone the repository:

    ```bash
    git clone https://github.com/th3l3gend2777/FerrisUnzip/
    cd FerrisUnzip
    ```

2.  Build the project:

    ```bash
    cargo build --release
    ```

3.  The executable will be located at `target/release/FerrisUnzip`. You can copy it to a directory in your PATH for easy access.

## Usage

### GUI Mode (Default)

Simply run the executable without any arguments to launch the graphical interface:

```bash
./FerrisUnzip
# or
cargo run
```

The GUI provides:
- **Browse buttons** to select your archive file and extraction destination
- **Password field** for encrypted archives
- **Visual feedback** with status messages and progress indicators
- **Easy extraction** with a single click
- **Install button** to register shell integration (adds "Extract with FerrisUnzip" to context menus)

### Shell Integration

To enable right-click context menu integration:

1. Launch FerrisUnzip GUI
2. Click the "Install Shell Integration" button
3. On Windows: This adds registry entries for context menu integration
4. On Linux: This creates a .desktop file for file associations

After installation:
- **Windows**: Right-click any archive file → "Extract with FerrisUnzip"
- **Linux**: Archive files will show FerrisUnzip as an available application

### CLI Mode

To use the command-line interface, provide an archive file as an argument:

```bash
./FerrisUnzip <archive_file> [OPTIONS]
# or
cargo run <archive_file> [OPTIONS]
```

**Options:**
- `-p, --password <PASSWORD>`: Password for encrypted 7Z archives

**Example:**
```bash
./FerrisUnzip myarchive.zip
./FerrisUnzip protected.7z -p mypassword
```

# Dependencies

## Core Libraries
-   **clap:** Command-line argument parsing for CLI mode.
-   **zip:** ZIP archive extraction.
-   **sevenz-rust:** 7Z archive extraction with AES-256 encryption support.
-   **tar:** TAR archive extraction.
-   **flate2:** GZ decompression.
-   **bzip2:** BZ2 decompression.
-   **liblzma:** XZ decompression.
-   **unrar:** RAR archive extraction.

## GUI Libraries
-   **slint:** Declarative UI framework for building native GUIs.
-   **slint-build:** Build-time compiler for .slint UI files.
-   **rfd:** Native file picker dialogs for all platforms.

## Supported Platforms

FerrisUnzip's GUI works on:
- **Windows** (Windows 7 and later)
- **macOS** (10.13 High Sierra and later)
- **Linux** (with X11 or Wayland)

The CLI mode works on any platform where Rust can compile.

# Contributing

Contributions are welcome! Please feel free to submit a pull request or open an issue.
# License

This project is licensed under the GPL License.
