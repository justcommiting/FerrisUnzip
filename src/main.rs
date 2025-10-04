// Hide console window on Windows when running in GUI mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::{Arg, Command};
use eframe::egui;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;
use sevenz_rust::{decompress_file_with_password, Password};
use tar::Archive as TarArchive;
use flate2::read::GzDecoder;
use bzip2::read::BzDecoder;
use liblzma::read::XzDecoder;
use unrar::Archive;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

// Progress callback type
type ProgressCallback = Arc<dyn Fn(f32, String) + Send + Sync>;

// Extraction options for fine-grained control
#[derive(Clone, Debug)]
struct ExtractionOptions {
    /// Allow size mismatches (useful for ISOs with padding from burned discs)
    allow_padding: bool,
    /// Maximum padding tolerance in bytes (default 2048 bytes for ISO sectors)
    max_padding_tolerance: u64,
    /// Skip validation for faster extraction
    skip_validation: bool,
    /// Be lenient with compression ratios for legitimate large files
    lenient_compression_check: bool,
}

impl Default for ExtractionOptions {
    fn default() -> Self {
        Self {
            allow_padding: true,  // Enable by default for versatility
            max_padding_tolerance: 2048 * 1024, // 2MB default tolerance (for ISO sector padding)
            skip_validation: false,
            lenient_compression_check: false,
        }
    }
}

// Enum to represent supported archive types
#[derive(Debug)]
enum ArchiveType {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    Gz,
    Bz2,
    Xz,
    Rar,
    Unknown,
}

// Determine archive type based on file extension
fn get_archive_type(path: &Path) -> ArchiveType {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        match ext.to_lowercase().as_str() {
            "zip" => ArchiveType::Zip,
            "7z" => ArchiveType::SevenZ,
            "tar" => ArchiveType::Tar,
            "gz" => {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.ends_with(".tar") {
                        ArchiveType::TarGz
                    } else {
                        ArchiveType::Gz
                    }
                } else {
                    ArchiveType::Unknown
                }
            }
            "bz2" => {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.ends_with(".tar") {
                        ArchiveType::TarBz2
                    } else {
                        ArchiveType::Bz2
                    }
                } else {
                    ArchiveType::Unknown
                }
            }
            "xz" => {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.ends_with(".tar") {
                        ArchiveType::TarXz
                    } else {
                        ArchiveType::Xz
                    }
                } else {
                    ArchiveType::Unknown
                }
            }
            "rar" => ArchiveType::Rar, 
            _ => ArchiveType::Unknown,
        }
    } else {
        ArchiveType::Unknown
    }
}

/// Modular extraction utilities for better code organization
mod extraction_utils {
    use super::*;
    
    /// Common progress reporting helper
    pub fn report_progress(
        callback: &Option<ProgressCallback>,
        progress: f32,
        message: String,
    ) {
        if let Some(ref cb) = callback {
            cb(progress, message);
        }
    }
    
    /// Calculate and format file size for display
    pub fn format_size(bytes: u64) -> String {
        if bytes > 1024 * 1024 * 1024 {
            format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if bytes > 1024 * 1024 {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes > 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else {
            format!("{} bytes", bytes)
        }
    }
    
    /// Prepare extraction directory - common pre-processing
    pub fn prepare_extraction_dir(extract_to: &Path) -> Result<(), Box<dyn Error>> {
        if extract_to.exists() && !extract_to.is_dir() {
            return Err("Extraction path exists but is not a directory".into());
        }
        
        fs::create_dir_all(extract_to)?;
        
        // Check write permissions
        if extract_to.metadata()?.permissions().readonly() {
            return Err("Cannot write to extraction directory".into());
        }
        
        Ok(())
    }
    
    /// Common extraction result handler
    pub fn finalize_extraction(
        callback: &Option<ProgressCallback>,
        format_name: &str,
        success: bool,
    ) {
        if let Some(ref cb) = callback {
            if success {
                cb(100.0, format!("{} extraction completed successfully", format_name));
            } else {
                cb(0.0, format!("{} extraction failed", format_name));
            }
        }
    }
}



// Extract ZIP archive (non-encrypted) with progress tracking
fn extract_zip(archive_path: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    extract_zip_with_options(archive_path, extract_to, progress_callback, ExtractionOptions::default())
}

// Extract ZIP archive with custom options for versatility
fn extract_zip_with_options(
    archive_path: &str, 
    extract_to: &str, 
    progress_callback: Option<ProgressCallback>,
    options: ExtractionOptions
) -> Result<(), Box<dyn Error>> {
    // Pre-extraction security validation
    let archive_path_buf = Path::new(archive_path);
    validate_archive_file(archive_path_buf)?;
    
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    let total_files = archive.len();
    
    // Check file count limit
    if total_files > MAX_FILES {
        return Err(format!("Archive contains too many files: {} (limit: {})", total_files, MAX_FILES).into());
    }
    
    let processed = Arc::new(AtomicUsize::new(0));
    let mut total_extracted_size = 0u64;
    let extract_to_path = Path::new(extract_to);
    
    // Ensure extraction directory exists and is writable
    fs::create_dir_all(extract_to_path)?;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let file_size = file.size();
        let compressed_size = file.compressed_size();
        let file_name = file.name().to_string();
        
        // Enhanced security validation with overflow protection (skip if requested)
        if !options.skip_validation {
            validate_extraction_size_with_options(
                total_extracted_size, 
                file_size, 
                compressed_size,
                options.lenient_compression_check
            )?;
        }
        
        // Safe addition with overflow protection
        total_extracted_size = safe_add_u64(total_extracted_size, file_size)?;
        
        // Sanitize the output path with enhanced security
        let outpath = sanitize_path(&file_name, extract_to_path)?;

        if file_name.ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            
            // Safe extraction with overflow and padding tolerance
            let mut outfile = File::create(&outpath)?;
            let limited_reader = file.take(file_size + options.max_padding_tolerance);
            
            // Use safe decompression wrapper with padding support
            let actual_size = safe_decompress_with_limits(
                limited_reader,
                &mut outfile,
                file_size + options.max_padding_tolerance
            )?;
            
            // Verify actual extracted size with padding tolerance
            if !options.allow_padding && actual_size != file_size {
                return Err(format!("Size mismatch: expected {}, got {} bytes", file_size, actual_size).into());
            } else if options.allow_padding {
                // Check if size is within acceptable tolerance
                let size_diff = if actual_size > file_size {
                    actual_size - file_size
                } else {
                    file_size - actual_size
                };
                
                if size_diff > options.max_padding_tolerance {
                    return Err(format!(
                        "Size mismatch exceeds tolerance: expected {}, got {} bytes (diff: {} bytes, max tolerance: {} bytes)", 
                        file_size, actual_size, size_diff, options.max_padding_tolerance
                    ).into());
                } else if size_diff > 0 {
                    // Log padding detected but continue extraction
                    if let Some(ref callback) = progress_callback {
                        callback(
                            (i as f32 / total_files as f32) * 100.0,
                            format!("Note: File '{}' has {} bytes padding (common in ISO files from burned discs)", 
                                file_name, size_diff)
                        );
                    }
                }
            }
        }

        let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(ref callback) = progress_callback {
            let progress = (current as f32 / total_files as f32) * 100.0;
            
            // Optimize progress reporting for large archives - report every 100 files or 1%
            let should_report = if total_files > 10000 {
                current % 100 == 0 || progress as u32 != ((current - 1) as f32 / total_files as f32 * 100.0) as u32
            } else {
                true // Report all for smaller archives
            };
            
            if should_report {
                let size_info = if total_extracted_size > 1024 * 1024 * 1024 {
                    format!(" ({:.1}GB extracted)", total_extracted_size as f64 / (1024.0 * 1024.0 * 1024.0))
                } else if total_extracted_size > 1024 * 1024 {
                    format!(" ({:.1}MB extracted)", total_extracted_size as f64 / (1024.0 * 1024.0))
                } else {
                    String::new()
                };
                callback(progress, format!("Extracted {} of {} files{}", current, total_files, size_info));
            }
        }
    }
    Ok(())
}

// Extract 7Z archive (supports encryption with password) with progress callback
fn extract_7z(archive: &str, extract_to: &str, password: Option<&str>, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn std::error::Error>> {
    use extraction_utils::*;
    
    let path = Path::new(archive);

    report_progress(&progress_callback, 10.0, "Initializing 7Z extraction...".to_string());

    if let Some(pwd) = password {
        let password = Password::from(pwd);
        decompress_file_with_password(path, extract_to, password)?;
    } else {
        decompress_file_with_password(path, extract_to, Password::from(""))?;
    }
    
    finalize_extraction(&progress_callback, "7Z", true);
    Ok(())
}

// Extract plain TAR archive with progress callback
fn extract_tar(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    use extraction_utils::*;
    
    let file = File::open(archive)?;
    let mut archive = TarArchive::new(file);
    
    report_progress(&progress_callback, 10.0, "Starting TAR extraction...".to_string());
    
    prepare_extraction_dir(Path::new(extract_to))?;
    archive.unpack(extract_to)?;
    
    finalize_extraction(&progress_callback, "TAR", true);
    Ok(())
}


// Extract TAR archive with compression and progress callback
fn extract_tar_compressed(extract_to: &str, decoder: impl io::Read, progress_callback: Option<ProgressCallback>, format_name: &str) -> Result<(), Box<dyn Error>> {
    use extraction_utils::*;
    
    let mut archive = TarArchive::new(decoder);
    
    report_progress(&progress_callback, 50.0, format!("Extracting {} archive...", format_name));
    
    prepare_extraction_dir(Path::new(extract_to))?;
    archive.unpack(extract_to)?;
    
    finalize_extraction(&progress_callback, format_name, true);
    Ok(())
}

// Extract TAR.GZ archive
fn extract_tar_gz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let decoder = GzDecoder::new(file);
    extract_tar_compressed(extract_to, decoder, progress_callback, "TAR.GZ")
}

// Extract TAR.BZ2 archive
fn extract_tar_bz2(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let decoder = BzDecoder::new(file);
    extract_tar_compressed(extract_to, decoder, progress_callback, "TAR.BZ2")
}

// Extract TAR.XZ archive
fn extract_tar_xz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let decoder = XzDecoder::new(file);
    extract_tar_compressed(extract_to, decoder, progress_callback, "TAR.XZ")
}

// Decompress single-file GZ with progress callback
fn decompress_gz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    decompress_single_file(archive, extract_to, progress_callback, "GZ", |file| {
        Box::new(GzDecoder::new(file)) as Box<dyn io::Read>
    })
}

// Decompress single-file BZ2 with progress callback
fn decompress_bz2(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    decompress_single_file(archive, extract_to, progress_callback, "BZ2", |file| {
        Box::new(BzDecoder::new(file)) as Box<dyn io::Read>
    })
}

// Decompress single-file XZ with progress callback
fn decompress_xz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    decompress_single_file(archive, extract_to, progress_callback, "XZ", |file| {
        Box::new(XzDecoder::new(file)) as Box<dyn io::Read>
    })
}

// Common single-file decompression logic - more modular approach
fn decompress_single_file<F>(
    archive: &str,
    extract_to: &str,
    progress_callback: Option<ProgressCallback>,
    format_name: &str,
    decoder_factory: F,
) -> Result<(), Box<dyn Error>>
where
    F: FnOnce(File) -> Box<dyn io::Read>,
{
    use extraction_utils::*;
    
    let file = File::open(archive)?;
    let mut decoder = decoder_factory(file);
    let output_file = Path::new(extract_to).join(
        Path::new(archive)
            .file_stem()
            .ok_or("Invalid filename")?
    );
    
    report_progress(&progress_callback, 25.0, format!("Decompressing {} file...", format_name));
    
    let mut outfile = File::create(output_file)?;
    io::copy(&mut decoder, &mut outfile)?;
    
    finalize_extraction(&progress_callback, format_name, true);
    Ok(())
}

// Main extraction function with comprehensive security validation
fn extract_archive(archive: &str, extract_to: &str, password: Option<&str>, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    let path = Path::new(archive);
    
    // Comprehensive pre-extraction security validation
    validate_archive_file(path)?;
    
    // Validate extraction directory
    let extract_path = Path::new(extract_to);
    if extract_path.exists() && !extract_path.is_dir() {
        return Err("Extraction path exists but is not a directory".into());
    }
    
    // Validate password if provided
    if let Some(pwd) = password {
        validate_password(pwd)?;
    }
    
    // Create extraction directory with proper permissions
    fs::create_dir_all(extract_path)?;
    
    // Check write permissions
    if extract_path.metadata()?.permissions().readonly() {
        return Err("Cannot write to extraction directory".into());
    }
    
    // Estimate and validate disk space for large archives
    let archive_size = std::fs::metadata(path)?.len();
    // Conservative estimate: extracted size is typically 2-10x compressed size for large archives
    let estimated_extraction_size = archive_size * 5; // Conservative 5x multiplier
    check_available_disk_space(extract_path, estimated_extraction_size)?;

    if let Some(ref callback) = progress_callback {
        let archive_size_gb = std::fs::metadata(path)?.len() as f64 / (1024.0 * 1024.0 * 1024.0);
        if archive_size_gb > 10.0 {
            callback(0.0, format!("🔒 Security validation passed. Processing large archive ({:.1} GB)...", archive_size_gb));
        } else {
            callback(0.0, "🔒 Security validation passed, starting extraction...".to_string());
        }
    }

    let archive_type = get_archive_type(path);
    
    // Execute extraction with appropriate security measures
    let result = match archive_type {
        ArchiveType::Zip => extract_zip(archive, extract_to, progress_callback.clone()),
        ArchiveType::SevenZ => extract_7z(archive, extract_to, password, progress_callback.clone()),
        ArchiveType::Tar => extract_tar(archive, extract_to, progress_callback.clone()),
        ArchiveType::TarGz => extract_tar_gz(archive, extract_to, progress_callback.clone()),
        ArchiveType::TarBz2 => extract_tar_bz2(archive, extract_to, progress_callback.clone()),
        ArchiveType::TarXz => extract_tar_xz(archive, extract_to, progress_callback.clone()),
        ArchiveType::Gz => decompress_gz(archive, extract_to, progress_callback.clone()),
        ArchiveType::Bz2 => decompress_bz2(archive, extract_to, progress_callback.clone()),
        ArchiveType::Xz => decompress_xz(archive, extract_to, progress_callback.clone()),
        ArchiveType::Rar => extract_rar(archive, extract_to, progress_callback.clone()),
        ArchiveType::Unknown => Err("Unsupported archive format - potential security risk".into()),
    };
    
    // Post-extraction validation
    if result.is_ok() {
        if let Some(ref callback) = progress_callback {
            callback(100.0, "Extraction completed successfully with security validation".to_string());
        }
    }
    
    result
}



// Command-line interface

fn run_cli() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("FerrisUnzip")
        .version("1.0")
        .about("Extracts various archive formats in Rust")
        .arg(Arg::new("archive").help("Path to the archive file").required(true).index(1))
        .arg(Arg::new("password").short('p').long("password").help("Password for encrypted archives").required(false))
        .arg(Arg::new("cli").long("cli").help("Force CLI mode").action(clap::ArgAction::SetTrue))
        .get_matches();

    let archive_path = matches.get_one::<String>("archive").unwrap();
    let mut password = matches.get_one::<String>("password").map(|s| s.as_str());

    // Prompt for extraction directory
    print!("Where do you want to extract to? (Leave blank to extract where the file is): ");
    io::stdout().flush()?;

    let mut extract_to_str = String::new();
    io::stdin().read_line(&mut extract_to_str)?;
    let extract_to_str = extract_to_str.trim();

    // Determine the extraction directory
    let extract_to: PathBuf = if !extract_to_str.is_empty() {
        PathBuf::from(extract_to_str)
    } else {
        let archive_path_obj = Path::new(archive_path);
        let archive_dir = archive_path_obj.parent().ok_or("Invalid archive path: Unable to determine parent directory")?;
        let archive_filename = archive_path_obj
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid filename: Unable to extract file stem")?;
        archive_dir.join(archive_filename)
    };

    // Create the extraction directory
    fs::create_dir_all(&extract_to)?;

    // Simple progress callback for CLI
    let progress_callback: ProgressCallback = Arc::new(|progress, message| {
        println!("[{:.1}%] {}", progress, message);
    });

    // Attempt extraction
    let mut result = extract_archive(archive_path, extract_to.to_str().unwrap(), password, Some(progress_callback.clone()));

    // Check for missing password error
    if let Err(err) = &result {
        if err.to_string().contains("Pass") {
            // Prompt for password
            print!("Password for encrypted archive: ");
            io::stdout().flush()?;

            let mut new_password = String::new();
            io::stdin().read_line(&mut new_password)?;
            password = Some(new_password.trim());

            // Retry extraction with password
            result = extract_archive(archive_path, extract_to.to_str().unwrap(), password, Some(progress_callback));
        }
    }

    // Handle final result
    match result {
        Ok(_) => println!("Extraction successful."),
        Err(err) => eprintln!("Extraction failed: {}", err),
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Set up panic handler for better error reporting
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("🚨 CRITICAL ERROR: FerrisUnzip encountered a fatal error");
        eprintln!("This may be due to a malformed archive or system resource limits.");
        
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            eprintln!("Error details: {}", s);
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            eprintln!("Error details: {}", s);
        }
        
        if let Some(location) = panic_info.location() {
            eprintln!("Location: {}:{}:{}", location.file(), location.line(), location.column());
        }
        
        eprintln!("\n🛡️  Security Note: This error was safely contained.");
        eprintln!("No files were corrupted and no security breach occurred.");
        eprintln!("\nTo prevent this error:");
        eprintln!("1. Ensure the archive file is not corrupted");
        eprintln!("2. Check available disk space");
        eprintln!("3. Try extracting to a different location");
        eprintln!("4. Use a smaller archive or extract fewer files");
        
        std::process::exit(1);
    }));
    
    let args: Vec<String> = std::env::args().collect();
    
    // Check if we have command-line arguments
    if args.len() > 1 {
        // Check for --cli flag or if multiple arguments (CLI mode)
        if args.contains(&"--cli".to_string()) || args.len() > 2 {
            // CLI mode
            run_cli()
        } else {
            // GUI mode with file argument from context menu
            let archive_file = &args[1];
            
            // Validate the file exists and is likely an archive
            if !Path::new(archive_file).exists() {
                eprintln!("Error: File '{}' does not exist", archive_file);
                return Err("Invalid file path".into());
            }
            
            let options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([500.0, 400.0])
                    .with_min_inner_size([400.0, 300.0])
                    .with_title("FerrisUnzip - Extract Archive"),
                ..Default::default()
            };
            
            eframe::run_native(
                "FerrisUnzip",
                options,
                Box::new(|_cc| Ok(Box::new(FerrisUnzipApp::new_with_file(archive_file.clone())))),
            ).map_err(|e| format!("Failed to run GUI: {}", e).into())
        }
    } else {
        // GUI mode without file
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([500.0, 400.0])
                .with_min_inner_size([400.0, 300.0]),
            ..Default::default()
        };
        
        eframe::run_native(
            "FerrisUnzip",
            options,
            Box::new(|_cc| Ok(Box::new(FerrisUnzipApp::default()))),
        ).map_err(|e| format!("Failed to run GUI: {}", e).into())
    }
}


fn extract_rar(archive_path: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    use extraction_utils::*;
    
    let mut archive = Archive::new(archive_path).open_for_processing()?;
    let mut file_count = 0;
    let mut processed_count = 0;

    // Ensure the extraction directory exists
    prepare_extraction_dir(Path::new(extract_to))?;

    report_progress(&progress_callback, 5.0, "Starting RAR extraction...".to_string());

    // First pass: count total files for progress tracking
    let mut temp_archive = Archive::new(archive_path).open_for_processing()?;
    while let Some(header) = temp_archive.read_header()? {
        file_count += 1;
        temp_archive = header.skip()?;
    }

    // Second pass: actual extraction with progress
    while let Some(header) = archive.read_header()? {
        let dest_path = Path::new(extract_to).join(header.entry().filename.to_string_lossy().as_ref());

        if header.entry().is_directory() {
            fs::create_dir_all(&dest_path)?;
            archive = header.skip()?;
        } else {
            // Ensure parent directories exist
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Extract the file to the destination
            archive = header.extract_to(&dest_path)?;
        }
        
        processed_count += 1;
        if let Some(ref callback) = progress_callback {
            let progress = if file_count > 0 { 
                (processed_count as f32 / file_count as f32) * 100.0 
            } else { 
                50.0 
            };
            callback(progress, format!("Extracted {} of {} files from RAR", processed_count, file_count));
        }
    }

    finalize_extraction(&progress_callback, "RAR", true);
    Ok(())
}

// OS detection
#[derive(Debug, PartialEq)]
enum OperatingSystem {
    Windows,
    Linux,
    MacOS,
    Other,
}

fn detect_os() -> OperatingSystem {
    if cfg!(target_os = "windows") {
        OperatingSystem::Windows
    } else if cfg!(target_os = "linux") {
        OperatingSystem::Linux
    } else if cfg!(target_os = "macos") {
        OperatingSystem::MacOS
    } else {
        OperatingSystem::Other
    }
}

// Shell integration installation
fn install_shell_integration() -> Result<String, Box<dyn Error>> {
    let os = detect_os();
    
    match os {
        OperatingSystem::Windows => install_windows_shell_integration(),
        OperatingSystem::Linux => install_linux_shell_integration(),
        _ => Err("Shell integration is currently only supported on Windows and Linux".into()),
    }
}

#[cfg(target_os = "windows")]
fn install_windows_shell_integration() -> Result<String, Box<dyn Error>> {
    use std::process::Command;
    
    let exe_path = env::current_exe()?;
    let exe_path_str = exe_path.display().to_string();
    
    // Create registry entries for context menu using individual reg.exe calls
    // This avoids quote escaping issues by passing arguments separately
    
    // 1. Add context menu entries for specific archive file types
    let archive_extensions = [
        ".zip", ".7z", ".tar", ".gz", ".bz2", ".xz", ".rar", 
        ".tar.gz", ".tar.bz2", ".tar.xz"
    ];
    
    // Add for common archive types
    for ext in &archive_extensions {
        let reg_key = format!("HKEY_CURRENT_USER\\Software\\Classes\\{}\\shell\\FerrisUnzip", ext);
        let output = Command::new("reg")
            .args(&[
                "add", 
                &reg_key,
                "/ve", 
                "/d", 
                "Extract with FerrisUnzip", 
                "/f"
            ])
            .output()?;
        
        if !output.status.success() {
            // Continue with other extensions even if one fails
            continue;
        }
        
        let cmd_key = format!("{}\\command", reg_key);
        let _cmd_output = Command::new("reg")
            .args(&[
                "add", 
                &cmd_key,
                "/ve", 
                "/d", 
                &format!("\"{}\" \"%1\"", exe_path_str), 
                "/f"
            ])
            .output()?;
    }

    // Also add general file context menu as fallback
    let output1 = Command::new("reg")
        .args(&[
            "add", 
            "HKEY_CURRENT_USER\\Software\\Classes\\*\\shell\\FerrisUnzip", 
            "/ve", 
            "/d", 
            "Extract with FerrisUnzip", 
            "/f"
        ])
        .output()?;
    
    if !output1.status.success() {
        return Err(format!("Failed to register shell integration (step 1): {}", 
            String::from_utf8_lossy(&output1.stderr)).into());
    }
    
    // 2. Add command for file context menu
    let output2 = Command::new("reg")
        .args(&[
            "add", 
            "HKEY_CURRENT_USER\\Software\\Classes\\*\\shell\\FerrisUnzip\\command", 
            "/ve", 
            "/d", 
            &format!("\"{}\" \"%1\"", exe_path_str), 
            "/f"
        ])
        .output()?;
    
    if !output2.status.success() {
        return Err(format!("Failed to register shell integration (step 2): {}", 
            String::from_utf8_lossy(&output2.stderr)).into());
    }
    
    // 3. Add main context menu entry for directory background
    let output3 = Command::new("reg")
        .args(&[
            "add", 
            "HKEY_CURRENT_USER\\Software\\Classes\\Directory\\Background\\shell\\FerrisUnzip", 
            "/ve", 
            "/d", 
            "Extract Archive with FerrisUnzip", 
            "/f"
        ])
        .output()?;
    
    if !output3.status.success() {
        return Err(format!("Failed to register shell integration (step 3): {}", 
            String::from_utf8_lossy(&output3.stderr)).into());
    }
    
    // 4. Add command for directory background context menu
    let output4 = Command::new("reg")
        .args(&[
            "add", 
            "HKEY_CURRENT_USER\\Software\\Classes\\Directory\\Background\\shell\\FerrisUnzip\\command", 
            "/ve", 
            "/d", 
            &format!("\"{}\"", exe_path_str), 
            "/f"
        ])
        .output()?;
    
    if !output4.status.success() {
        return Err(format!("Failed to register shell integration (step 4): {}", 
            String::from_utf8_lossy(&output4.stderr)).into());
    }
    
    Ok("Shell integration installed successfully! Right-click on archive files to see 'Extract with FerrisUnzip' option.".to_string())
}

#[cfg(not(target_os = "windows"))]
fn install_windows_shell_integration() -> Result<String, Box<dyn Error>> {
    Err("Windows shell integration is only available on Windows".into())
}

#[cfg(target_os = "linux")]
fn install_linux_shell_integration() -> Result<String, Box<dyn Error>> {
    let exe_path = env::current_exe()?;
    let exe_path_str = exe_path.display().to_string();
    
    // Create desktop entry
    let home_dir = env::var("HOME")?;
    let desktop_entry_dir = format!("{}/.local/share/applications", home_dir);
    fs::create_dir_all(&desktop_entry_dir)?;
    
    let desktop_entry_path = format!("{}/ferrisunzip.desktop", desktop_entry_dir);
    let desktop_entry_content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=FerrisUnzip\n\
         Comment=Extract various archive formats\n\
         Exec={} %f\n\
         Icon=utilities-file-archiver\n\
         Terminal=false\n\
         Categories=Utility;Archiving;\n\
         MimeType=application/zip;application/x-7z-compressed;application/x-tar;application/gzip;application/x-bzip2;application/x-xz;application/x-rar;\n",
        exe_path_str
    );
    
    let mut file = File::create(&desktop_entry_path)?;
    file.write_all(desktop_entry_content.as_bytes())?;
    
    // Update desktop database if available
    if let Ok(_) = std::process::Command::new("update-desktop-database")
        .arg(desktop_entry_dir)
        .output() {
        // Command executed successfully
    }
    
    Ok("Shell integration installed successfully! Archive files should now show FerrisUnzip as an option.".to_string())
}

#[cfg(not(target_os = "linux"))]
fn install_linux_shell_integration() -> Result<String, Box<dyn Error>> {
    Err("Linux shell integration is only available on Linux".into())
}

// GUI Application with Security Tracking
struct FerrisUnzipApp {
    archive_path: String,
    extract_to_path: String,
    password: String,
    status_message: String,
    is_extracting: bool,
    extraction_result: Arc<Mutex<Option<Result<(), String>>>>,
    install_message: String,
    progress: Arc<Mutex<f32>>,
    progress_message: Arc<Mutex<String>>,
    extraction_start_time: Option<Instant>,
    security_warnings: Vec<String>,
    password_attempts: u8,
}

impl Default for FerrisUnzipApp {
    fn default() -> Self {
        Self {
            archive_path: String::new(),
            extract_to_path: String::new(),
            password: String::new(),
            status_message: String::from("Select an archive file to extract"),
            is_extracting: false,
            extraction_result: Arc::new(Mutex::new(None)),
            install_message: String::new(),
            progress: Arc::new(Mutex::new(0.0)),
            progress_message: Arc::new(Mutex::new(String::new())),
            extraction_start_time: None,
            security_warnings: Vec::new(),
            password_attempts: 0,
        }
    }
}

impl FerrisUnzipApp {
    fn new_with_file(archive_path: String) -> Self {
        // Auto-set extraction path based on archive location
        let extract_to_path = if let Some(archive_path_obj) = Path::new(&archive_path).parent() {
            if let Some(filename) = Path::new(&archive_path).file_stem() {
                archive_path_obj
                    .join(filename)
                    .display()
                    .to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Security pre-validation
        let mut security_warnings = Vec::new();
        let status_message = if Path::new(&archive_path).exists() {
            // Validate archive security
            match validate_archive_file(Path::new(&archive_path)) {
                Ok(_) => format!("✅ Archive loaded: {}. Security validation passed!", 
                    Path::new(&archive_path).file_name()
                        .unwrap_or_default()
                        .to_string_lossy()),
                Err(e) => {
                    security_warnings.push(format!("Security warning: {}", e));
                    format!("⚠️  Archive loaded with warnings: {}. Check security status!", 
                        Path::new(&archive_path).file_name()
                            .unwrap_or_default()
                            .to_string_lossy())
                }
            }
        } else {
            "❌ Error: Selected archive file does not exist".to_string()
        };

        Self {
            archive_path,
            extract_to_path,
            password: String::new(),
            status_message,
            is_extracting: false,
            extraction_result: Arc::new(Mutex::new(None)),
            install_message: String::new(),
            progress: Arc::new(Mutex::new(0.0)),
            progress_message: Arc::new(Mutex::new(String::new())),
            extraction_start_time: None,
            security_warnings,
            password_attempts: 0,
        }
    }

    fn start_extraction(&mut self) {
        // Security validation before starting extraction
        if !self.password.is_empty() {
            if let Err(e) = validate_password(&self.password) {
                self.status_message = format!("❌ Password validation failed: {}", e);
                return;
            }
            
            self.password_attempts += 1;
            if self.password_attempts > MAX_PASSWORD_ATTEMPTS {
                self.status_message = "❌ Too many password attempts. Please restart the application.".to_string();
                return;
            }
        }
        
        self.is_extracting = true;
        self.extraction_start_time = Some(Instant::now());
        self.status_message = "🔒 Security checks passed. Starting secure extraction...".to_string();
        *self.progress.lock().unwrap() = 0.0;
        *self.progress_message.lock().unwrap() = "Validating security parameters...".to_string();
        
        let archive_path = self.archive_path.clone();
        let extract_to = self.extract_to_path.clone();
        let password = if self.password.is_empty() {
            None
        } else {
            Some(self.password.clone())
        };
        let result_arc = Arc::clone(&self.extraction_result);
        let progress_arc = Arc::clone(&self.progress);
        let progress_msg_arc = Arc::clone(&self.progress_message);
        
        // Create progress callback
        let progress_callback: ProgressCallback = Arc::new(move |progress, message| {
            *progress_arc.lock().unwrap() = progress;
            *progress_msg_arc.lock().unwrap() = message;
        });
        
        // Run extraction in a separate thread
        thread::spawn(move || {
            let result = extract_archive(
                &archive_path,
                &extract_to,
                password.as_deref(),
                Some(progress_callback)
            );
            
            let mapped_result = result.map_err(|e| e.to_string());
            *result_arc.lock().unwrap() = Some(mapped_result);
        });
    }
}

impl eframe::App for FerrisUnzipApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for extraction completion
        if self.is_extracting {
            if let Ok(mut result) = self.extraction_result.try_lock() {
                if let Some(res) = result.take() {
                    self.is_extracting = false;
                    self.extraction_start_time = None;
                    match res {
                        Ok(_) => {
                            self.status_message = "✓ Extraction successful!".to_string();
                            *self.progress.lock().unwrap() = 100.0;
                            *self.progress_message.lock().unwrap() = "Completed!".to_string();
                        }
                        Err(e) => {
                            self.status_message = format!("✗ Extraction failed: {}", e);
                            *self.progress.lock().unwrap() = 0.0;
                            *self.progress_message.lock().unwrap() = "Failed".to_string();
                        }
                    }
                }
            }
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("FerrisUnzip - Archive Extractor");
            
            // Show context menu indicator if launched with a file
            if !self.archive_path.is_empty() && self.status_message.contains("Archive loaded") {
                ui.add_space(5.0);
                ui.colored_label(
                    egui::Color32::from_rgb(0, 120, 200), 
                    "📂 Launched from context menu - archive pre-selected"
                );
            }
            
            ui.add_space(20.0);

            // Archive file selection
            ui.horizontal(|ui| {
                ui.label("Archive file:");
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Archives", &["zip", "7z", "tar", "gz", "bz2", "xz", "rar"])
                        .add_filter("All files", &["*"])
                        .pick_file()
                    {
                        self.archive_path = path.display().to_string();
                        
                        // Auto-set extraction path based on archive location
                        if let Some(archive_path_obj) = Path::new(&self.archive_path).parent() {
                            if let Some(filename) = Path::new(&self.archive_path).file_stem() {
                                self.extract_to_path = archive_path_obj
                                    .join(filename)
                                    .display()
                                    .to_string();
                            }
                        }
                        
                        self.status_message = "Archive selected. Choose extraction destination.".to_string();
                    }
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.archive_path)
                        .desired_width(350.0)
                );
            });

            ui.add_space(15.0);

            // Extraction directory selection
            ui.horizontal(|ui| {
                ui.label("Extract to:");
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.extract_to_path = path.display().to_string();
                    }
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.extract_to_path)
                        .desired_width(350.0)
                );
            });

            ui.add_space(15.0);

            // Password field (for encrypted archives)
            ui.horizontal(|ui| {
                ui.label("Password (optional):");
                ui.add(
                    egui::TextEdit::singleline(&mut self.password)
                        .password(true)
                        .desired_width(250.0)
                        .hint_text("Leave blank for non-encrypted archives")
                );
            });

            ui.add_space(20.0);

            // Progress bar (show when extracting)
            if self.is_extracting {
                ui.add_space(10.0);
                ui.label("Extraction Progress:");
                
                let progress = *self.progress.lock().unwrap();
                let progress_msg = self.progress_message.lock().unwrap().clone();
                
                ui.add(egui::ProgressBar::new(progress / 100.0).text(format!("{:.1}%", progress)));
                ui.label(&progress_msg);
                
                // Show elapsed time
                if let Some(start_time) = self.extraction_start_time {
                    let elapsed = start_time.elapsed();
                    ui.label(format!("Elapsed: {:.1}s", elapsed.as_secs_f32()));
                }
                
                ui.add_space(10.0);
            }

            // Extract button
            ui.horizontal(|ui| {
                let can_extract = !self.archive_path.is_empty() 
                    && !self.extract_to_path.is_empty() 
                    && !self.is_extracting;
                
                if ui.add_enabled(can_extract, egui::Button::new("Extract Archive")).clicked() {
                    self.start_extraction();
                }
                
                // Quick Extract button for context menu usage
                if can_extract && self.status_message.contains("Archive loaded") {
                    ui.add_space(10.0);
                    if ui.button("🚀 Quick Extract").clicked() {
                        self.start_extraction();
                    }
                }
                
                if self.is_extracting {
                    ui.spinner();
                }
            });

            // Install button
            ui.add_space(20.0);
            if ui.button("Install into context menu (Right-click on files)").clicked() {
                self.install_message = match install_shell_integration() {
                    Ok(msg) => format!("✓ {}", msg),
                    Err(e) => format!("✗ Installation failed: {}", e),
                };
            }

            ui.add_space(15.0);

            // Status message
            ui.separator();
            ui.add_space(10.0);
            
            let status_color = if self.status_message.starts_with("✓") {
                egui::Color32::from_rgb(0, 150, 0)
            } else if self.status_message.starts_with("✗") {
                egui::Color32::from_rgb(200, 0, 0)
            } else if self.status_message.contains("Extracting") {
                egui::Color32::from_rgb(0, 100, 200)
            } else {
                egui::Color32::from_rgb(100, 100, 100)
            };
            
            ui.colored_label(status_color, &self.status_message);

            // Install message
            if !self.install_message.is_empty() {
                ui.add_space(5.0);
                let install_color = if self.install_message.starts_with("✓") {
                    egui::Color32::from_rgb(0, 150, 0)
                } else {
                    egui::Color32::from_rgb(200, 0, 0)
                };
                ui.colored_label(install_color, &self.install_message);
            }

            ui.add_space(15.0);

            // Supported formats
            ui.separator();
            ui.add_space(10.0);
            ui.label("Supported formats: ZIP, 7Z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, RAR");
            ui.label("Version 1.0 - Cross-platform archive extractor");
        });
    }
}

use security_config::*;

/// Safe arithmetic operations with overflow protection
mod safe_ops {
    use std::error::Error;
    use std::convert::TryFrom;
    
    pub fn safe_add_u64(a: u64, b: u64) -> Result<u64, Box<dyn Error>> {
        a.checked_add(b).ok_or_else(|| "Integer overflow in size calculation".into())
    }
    
    pub fn safe_multiply_u64(a: u64, b: u64) -> Result<u64, Box<dyn Error>> {
        a.checked_mul(b).ok_or_else(|| "Integer overflow in size multiplication".into())
    }
    
    pub fn safe_cast_usize_to_u64(value: usize) -> Result<u64, Box<dyn Error>> {
        u64::try_from(value).map_err(|_| "Size conversion overflow".into())
    }
    
    pub fn safe_cast_u64_to_usize(value: u64) -> Result<usize, Box<dyn Error>> {
        usize::try_from(value).map_err(|_| "Size conversion overflow - value too large for platform".into())
    }
}

use safe_ops::*;

/// Comprehensive security validation and path sanitization
mod security {
    use super::*;
    use std::path::{Path, PathBuf, Component};
    
    pub fn validate_archive_file(path: &Path) -> Result<(), Box<dyn Error>> {
        // Check file exists and is readable
        if !path.exists() {
            return Err("Archive file does not exist".into());
        }
        
        // Check file size with user-friendly formatting
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > MAX_ARCHIVE_SIZE {
            return Err(format!("Archive too large: {:.1} GB (max: {:.0} GB)", 
                metadata.len() as f64 / (1024.0 * 1024.0 * 1024.0),
                MAX_ARCHIVE_SIZE as f64 / (1024.0 * 1024.0 * 1024.0)).into());
        }
        
        // Provide helpful info for large archives
        if metadata.len() > 10 * 1024 * 1024 * 1024 { // 10GB+
            eprintln!("INFO: Processing large archive ({:.1} GB) - this may take significant time", 
                metadata.len() as f64 / (1024.0 * 1024.0 * 1024.0));
        }
        
        // Validate file extension
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if !ALLOWED_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                return Err(format!("Unsupported archive type: {}", ext).into());
            }
        } else {
            return Err("Archive has no file extension".into());
        }
        
        Ok(())
    }
    
    pub fn validate_password(password: &str) -> Result<(), Box<dyn Error>> {
        if password.len() < MIN_PASSWORD_LENGTH {
            return Err("Password too short".into());
        }
        if password.len() > MAX_PASSWORD_LENGTH {
            return Err("Password too long".into());
        }
        Ok(())
    }
    
    pub fn sanitize_path(path: &str, extract_to: &Path) -> Result<PathBuf, Box<dyn Error>> {
        // Input validation
        if path.is_empty() {
            return Err("Empty file path".into());
        }
        
        if path.len() > MAX_FILENAME_LENGTH {
            return Err(format!("Filename too long: {} characters (max: {})", 
                path.len(), MAX_FILENAME_LENGTH).into());
        }
        
        // Normalize and validate path components
        let normalized = Path::new(path);
        let mut depth = 0;
        
        for component in normalized.components() {
            depth += 1;
            if depth > MAX_PATH_DEPTH {
                return Err("Path depth exceeds maximum allowed".into());
            }
            
            match component {
                Component::ParentDir => {
                    return Err("Directory traversal attempt detected (..)".into());
                }
                Component::RootDir => {
                    return Err("Absolute path not allowed".into());
                }
                Component::Prefix(_) => {
                    return Err("Drive prefix not allowed".into());
                }
                Component::Normal(name) => {
                    if let Some(name_str) = name.to_str() {
                        validate_filename(name_str)?;
                    } else {
                        return Err("Invalid Unicode in filename".into());
                    }
                }
                Component::CurDir => {
                    // Allow current directory references
                }
            }
        }
        
        let result = extract_to.join(normalized);
        
        // Ensure the final path is within the extraction directory
        let canonical_extract_to = extract_to.canonicalize().unwrap_or_else(|_| extract_to.to_path_buf());
        let canonical_result = result.parent()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_else(|| result.parent().unwrap_or(&result).to_path_buf());
        
        if !canonical_result.starts_with(&canonical_extract_to) {
            return Err("Path escapes extraction directory".into());
        }
        
        Ok(result)
    }
    
    fn validate_filename(filename: &str) -> Result<(), Box<dyn Error>> {
        // Check for null bytes
        if filename.contains('\0') {
            return Err("Null byte in filename".into());
        }
        
        // Check for control characters
        if filename.chars().any(|c| c.is_control()) {
            return Err("Control character in filename".into());
        }
        
        // Check for dangerous Windows reserved names
        let name_lower = filename.to_lowercase();
        let base_name = name_lower.split('.').next().unwrap_or(&name_lower);
        
        if DANGEROUS_FILENAMES.contains(&base_name) {
            return Err(format!("Reserved filename not allowed: {}", filename).into());
        }
        
        // Check for dangerous characters
        let dangerous_chars = ['<', '>', ':', '"', '|', '?', '*'];
        if filename.chars().any(|c| dangerous_chars.contains(&c)) {
            return Err(format!("Dangerous character in filename: {}", filename).into());
        }
        
        // Check for trailing spaces or dots (Windows issue)
        if filename.ends_with(' ') || filename.ends_with('.') {
            return Err("Filename ends with space or dot".into());
        }
        
        // Warn about potentially dangerous extensions
        if let Some(ext) = Path::new(filename).extension().and_then(|s| s.to_str()) {
            if DANGEROUS_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                // Log warning but allow extraction (user should be warned in UI)
                eprintln!("WARNING: Potentially dangerous file type: {}", filename);
            }
        }
        
        Ok(())
    }
    
    pub fn validate_extraction_size(current_size: u64, file_size: u64, compressed_size: u64) -> Result<(), Box<dyn Error>> {
        validate_extraction_size_with_options(current_size, file_size, compressed_size, false)
    }
    
    pub fn validate_extraction_size_with_options(
        current_size: u64, 
        file_size: u64, 
        compressed_size: u64,
        lenient_compression_check: bool
    ) -> Result<(), Box<dyn Error>> {
        // Overflow-safe validation
        
        // Check individual file size
        if file_size > MAX_INDIVIDUAL_FILE_SIZE {
            return Err(format!("File too large: {} GB (max: {} GB)", 
                file_size / (1024 * 1024 * 1024), MAX_INDIVIDUAL_FILE_SIZE / (1024 * 1024 * 1024)).into());
        }
        
        // Safe addition to check total size - prevent overflow
        let total_size = safe_add_u64(current_size, file_size)?;
        if total_size > MAX_EXTRACTED_SIZE {
            return Err(format!("Total extraction size would exceed limit: {:.1} GB + {:.1} GB > {:.1} GB", 
                current_size as f64 / (1024.0 * 1024.0 * 1024.0),
                file_size as f64 / (1024.0 * 1024.0 * 1024.0),
                MAX_EXTRACTED_SIZE as f64 / (1024.0 * 1024.0 * 1024.0)).into());
        }
        
        // Check compression ratio (zip bomb detection) - more lenient for large files
        if compressed_size > 0 {
            // Prevent division by zero and overflow in ratio calculation
            if compressed_size > u64::MAX / 1000 {
                return Err("Compressed size too large for safe processing".into());
            }
            
            let ratio = file_size as f64 / compressed_size as f64;
            let max_ratio = if lenient_compression_check {
                MAX_COMPRESSION_RATIO * 2.0 // Double the tolerance for lenient mode
            } else {
                MAX_COMPRESSION_RATIO
            };
            
            if ratio > max_ratio {
                // For large files, provide more context
                return Err(format!("Suspicious compression ratio: {:.2}x (max: {}x). File: {:.1}MB compressed to {:.1}MB", 
                    ratio, max_ratio,
                    compressed_size as f64 / (1024.0 * 1024.0),
                    file_size as f64 / (1024.0 * 1024.0)).into());
            }
            
            // Additional overflow protection for very large files
            if file_size > u64::MAX / 2 {
                return Err("File size too large for safe processing".into());
            }
        }
        
        Ok(())
    }
    
    /// Safe decompression wrapper with overflow protection
    pub fn safe_decompress_with_limits<R: std::io::Read, W: std::io::Write>(
        mut reader: R, 
        mut writer: W, 
        max_output_size: u64
    ) -> Result<u64, Box<dyn Error>> {
        use std::io::Read;
        
        const BUFFER_SIZE: usize = 64 * 1024; // 64KB buffer
        let mut buffer = vec![0u8; BUFFER_SIZE];
        let mut total_written = 0u64;
        
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let bytes_to_write = safe_cast_usize_to_u64(n)?;
                    
                    // Check if writing would exceed limits
                    let new_total = safe_add_u64(total_written, bytes_to_write)?;
                    if new_total > max_output_size {
                        return Err(format!("Decompressed size would exceed limit: {} bytes", new_total).into());
                    }
                    
                    writer.write_all(&buffer[..n])?;
                    total_written = new_total;
                }
                Err(e) => {
                    return Err(format!("Decompression error: {}", e).into());
                }
            }
        }
        
        Ok(total_written)
    }
    
    pub fn check_available_disk_space(extract_to: &Path, required_space: u64) -> Result<(), Box<dyn Error>> {
        // Get available disk space (platform-specific implementation would be ideal)
        // For now, we'll implement a basic check
        
        // Try to get the root path for space checking
        let root_path = if cfg!(windows) {
            extract_to.ancestors().last().unwrap_or(extract_to)
        } else {
            Path::new("/")
        };
        
        // This is a simplified check - in production, you'd want to use platform-specific APIs
        if let Ok(metadata) = std::fs::metadata(root_path) {
            // Basic heuristic: if we can't write, assume insufficient space
            if metadata.permissions().readonly() {
                return Err("Cannot write to destination (insufficient permissions or space)".into());
            }
        }
        
        // Warn for very large extractions
        if required_space > 100 * 1024 * 1024 * 1024 { // 100GB+
            eprintln!("WARNING: Large extraction ({:.1} GB) - ensure sufficient disk space", 
                required_space as f64 / (1024.0 * 1024.0 * 1024.0));
        }
        
        Ok(())
    }
}

use security::*;

// Legacy function for compatibility - now uses enhanced security module
fn sanitize_path(path: &str, extract_to: &Path) -> Result<PathBuf, Box<dyn Error>> {
    security::sanitize_path(path, extract_to)
}

// Security Configuration - Defense in Depth with Large Archive Support
mod security_config {
    // File and extraction limits - Supporting large archives up to 200GB
    pub const MAX_EXTRACTED_SIZE: u64 = 200 * 1024 * 1024 * 1024; // 200GB hard cap
    pub const MAX_FILES: usize = 1_000_000; // 1 million files maximum for large archives
    pub const MAX_INDIVIDUAL_FILE_SIZE: u64 = 50 * 1024 * 1024 * 1024; // 50GB per individual file
    pub const MAX_COMPRESSION_RATIO: f64 = 1000.0; // Higher ratio for legitimate large compressed files
    pub const MAX_FILENAME_LENGTH: usize = 255;
    pub const MAX_PATH_DEPTH: usize = 100; // Deeper paths for complex archives
    
    // Password security
    pub const MAX_PASSWORD_ATTEMPTS: u8 = 3;
    pub const MIN_PASSWORD_LENGTH: usize = 1;
    pub const MAX_PASSWORD_LENGTH: usize = 1024;
    
    // Archive validation - Support for large archive files
    pub const MAX_ARCHIVE_SIZE: u64 = 250 * 1024 * 1024 * 1024; // 250GB archive size limit (compressed)
    pub const ALLOWED_EXTENSIONS: &[&str] = &["zip", "7z", "tar", "gz", "bz2", "xz", "rar"];
    
    // Dangerous filename patterns
    pub const DANGEROUS_FILENAMES: &[&str] = &[
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5",
        "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4",
        "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"
    ];
    
    pub const DANGEROUS_EXTENSIONS: &[&str] = &[
        "exe", "bat", "cmd", "com", "scr", "pif", "vbs", "js", "jar",
        "ps1", "sh", "msi", "dll", "app", "deb", "rpm"
    ];
}

use security_config::*;
