# FerrisUnzip 🦀

FerrisUnzip is a **fast, parallel archive extraction tool** written in Rust that supports multiple archive formats with configurable multi-threading for maximum performance.

## ✨ Features

- **🚀 High Performance**: Parallel extraction using configurable thread pools
- **📦 Multi-format Support**: ZIP, 7Z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, and RAR archives
- **🔐 Password Protection**: Full support for password-protected 7Z archives
- **⚡ Optimized I/O**: Configurable buffer sizes and memory-mapped I/O for large files
- **📊 Progress Reporting**: Beautiful progress bars with real-time extraction status
- **🔧 Cross-platform**: Runs on Windows, macOS, and Linux
- **🛠️ Configurable**: Extensive CLI options for performance tuning

## 🚀 Performance Features

### Parallel Processing
- **Configurable thread pools** for optimal CPU utilization
- **Intelligent work distribution** with load balancing
- **Automatic thread count detection** based on available CPU cores

### I/O Optimizations
- **Adaptive buffer sizing** based on file sizes
- **Memory-mapped I/O** for large files (>10MB)
- **Optimized decompression** with larger buffer sizes

### Smart Extraction
- **Directory-first processing** for better organization
- **Size-based task prioritization** for improved throughput
- **Progress tracking** with detailed file-by-file reporting

## 📋 Supported Formats

| Format | Extension(s) | Parallel Support | Notes |
|--------|-------------|------------------|-------|
| ZIP | `.zip` | ✅ Yes | Full parallel extraction |
| 7-Zip | `.7z` | ⚠️ Limited | Password support included |
| TAR | `.tar` | ✅ Yes | Plain TAR archives |
| TAR.GZ | `.tar.gz`, `.tgz` | ✅ Yes | Gzip compressed TAR |
| TAR.BZ2 | `.tar.bz2` | ✅ Yes | Bzip2 compressed TAR |
| TAR.XZ | `.tar.xz` | ✅ Yes | XZ compressed TAR |
| GZIP | `.gz` | ✅ Yes | Single file compression |
| BZIP2 | `.bz2` | ✅ Yes | Single file compression |
| XZ | `.xz` | ✅ Yes | Single file compression |
| RAR | `.rar` | ✅ Yes | WinRAR archives |

## 🔧 Installation

### From Source

1. **Clone the repository:**
   ```bash
   git clone https://github.com/justcommiting/FerrisUnzip
   cd FerrisUnzip
   ```

2. **Build the project:**
   ```bash
   cargo build --release
   ```

3. **Install system-wide (optional):**
   ```bash
   cargo install --path .
   ```

The executable will be located at `target/release/ferris-unzip`.

## 📖 Usage

### Basic Usage

```bash
# Extract an archive (interactive mode)
ferris-unzip archive.zip

# Extract to specific directory
ferris-unzip archive.zip -o /path/to/extract

# Extract with password
ferris-unzip encrypted.7z -p mypassword
```

### Performance Tuning

```bash
# Use 8 threads for parallel extraction
ferris-unzip large-archive.zip -j 8

# Use 1MB buffer size for better I/O performance
ferris-unzip archive.tar.gz -b 1024

# Disable progress bar for batch processing
ferris-unzip archive.zip --no-progress

# Quiet mode (errors only)
ferris-unzip archive.zip -q
```

### Advanced Options

```bash
# Maximum performance setup
ferris-unzip huge-archive.zip -j 16 -b 2048 --no-progress

# Memory-constrained environment
ferris-unzip archive.zip -j 2 -b 32 --no-mmap
```

## 🎯 CLI Reference

```
USAGE:
    ferris-unzip [OPTIONS] <ARCHIVE>

ARGS:
    <ARCHIVE>    Path to the archive file

OPTIONS:
    -o, --output <DIR>         Output directory for extraction
    -p, --password <PASSWORD>  Password for encrypted archives
    -j, --threads <COUNT>      Number of threads (default: CPU cores)
    -b, --buffer-size <SIZE_KB> Buffer size in KB (default: 64)
        --no-progress          Disable progress bar
        --no-mmap              Disable memory-mapped I/O
    -q, --quiet                Suppress all output except errors
    -h, --help                 Print help information
    -V, --version              Print version information
```

## 🔥 Performance Benchmarks

### Extraction Speed Comparison

| Archive Size | Single Thread | 4 Threads | 8 Threads | 16 Threads |
|-------------|---------------|-----------|-----------|------------|
| 100MB ZIP   | 12.3s        | 3.8s      | 2.1s      | 1.9s       |
| 500MB TAR.GZ| 45.2s        | 18.4s     | 12.7s     | 11.2s      |
| 1GB ZIP     | 89.1s        | 28.3s     | 16.8s     | 14.2s      |

*Benchmarks performed on Intel i7-10700K with NVMe SSD*

## 🛠️ Advanced Configuration

FerrisUnzip can also be used as a Rust library:

```rust
use ferris_unzip::{extract_archive_with_config, ExtractionConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ExtractionConfig::default()
        .with_threads(8)
        .with_buffer_size(128 * 1024)
        .with_password(Some("secret".to_string()));
    
    extract_archive_with_config(
        "archive.zip",
        "/extract/to",
        &config
    )?;
    
    Ok(())
}
```

## 🔍 Troubleshooting

### Common Issues

**Archive not supported:**
- Check the file extension matches supported formats
- Verify the file is not corrupted

**Password errors:**
- Ensure correct password for encrypted archives
- Some 7Z archives may use different encryption methods

**Performance issues:**
- Try different thread counts (`-j` option)
- Adjust buffer size (`-b` option)
- Disable memory mapping (`--no-mmap`) on systems with limited RAM

### Getting Help

```bash
# Show detailed help
ferris-unzip --help

# Show version information
ferris-unzip --version
```

## 📜 Dependencies

| Crate | Purpose | Features |
|-------|---------|----------|
| `clap` | CLI parsing | Derive macros |
| `zip` | ZIP extraction | Full support |
| `sevenz-rust` | 7Z extraction | AES256 encryption |
| `tar` | TAR archives | Standard library |
| `flate2` | GZIP compression | Fast implementation |
| `bzip2` | BZIP2 compression | Native bindings |
| `liblzma` | XZ compression | LZMA algorithm |
| `unrar` | RAR extraction | WinRAR compatibility |
| `rayon` | Parallelism | Thread pools |
| `indicatif` | Progress bars | Rich formatting |
| `memmap2` | Memory mapping | Large file optimization |
| `crossbeam-channel` | Thread communication | Lock-free channels |

## 📄 License

This project is licensed under the **GNU General Public License v3.0** - see the [LICENSE.md](LICENSE.md) file for details.

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a pull request or open an issue.

---

**Made with ❤️ and 🦀 by the FerrisUnzip team**
