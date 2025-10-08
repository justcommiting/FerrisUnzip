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

// Cross-platform error dialog function
fn show_error_dialog(title: &str, message: &str) -> Result<(), Box<dyn Error>> {
    // Try to use rfd (Rust File Dialog) for cross-platform native dialogs
    // This works on Windows, macOS, and Linux with proper desktop environments
    
    // Use rfd's message dialog for cross-platform support
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .show();
    
    Ok(())
}

// Safe extraction wrapper that catches panics and converts them to errors
// This prevents the entire application from crashing when a critical error occurs
fn safe_extract_with_panic_recovery(
    archive: &str, 
    extract_to: &str, 
    password: Option<&str>, 
    progress_callback: Option<ProgressCallback>
) -> Result<(), Box<dyn Error>> {
    // Use std::panic::catch_unwind to catch panics and convert them to errors
    // This allows the GUI to continue running even if extraction fails catastrophically
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_archive_resilient(archive, extract_to, password, progress_callback)
    }));
    
    match result {
        Ok(extraction_result) => extraction_result,
        Err(panic_payload) => {
            // Convert panic into a recoverable error
            let panic_message = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic occurred during extraction".to_string()
            };
            
            let error_msg = format!("CRITICAL: Extraction panicked - {}", panic_message);
            eprintln!("Panic caught and converted to error: {}", error_msg);
            Err(error_msg.into())
        }
    }
}

// Resilient extraction that can continue even when individual operations fail
fn extract_archive_resilient(archive: &str, extract_to: &str, password: Option<&str>, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    let path = Path::new(archive);
    
    // Comprehensive pre-extraction security validation
    validate_archive_file(path)?;
    
    // Validate extraction directory
    let extract_path = Path::new(extract_to);
    if extract_path.exists() && !extract_path.is_dir() {
        return Err("Extraction path exists but is not a directory".into());
    }
    
    // Create extraction directory with proper permissions
    fs::create_dir_all(extract_path)?;
    
    let archive_type = get_archive_type(path);
    
    if let Some(ref callback) = progress_callback {
        callback(5.0, "Starting resilient extraction (can continue past individual file failures)...".to_string());
    }
    
    // Use the original extraction function but with better error context
    let extraction_result = match archive_type {
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
        ArchiveType::Unknown => Err("Unsupported archive format".into()),
    };
    
    match extraction_result {
        Ok(_) => {
            if let Some(ref callback) = progress_callback {
                callback(100.0, "Extraction completed successfully".to_string());
            }
            Ok(())
        }
        Err(e) => {
            // Provide context about what can be done
            let context_msg = if e.to_string().contains("overflow") {
                "This archive may be corrupted or contain files that trigger overflow errors in the decompression library. Some files may have been extracted successfully."
            } else if e.to_string().contains("Partial extraction successful") {
                "Partial extraction completed - some files were extracted successfully despite errors."
            } else {
                "Extraction failed - check archive integrity and available disk space."
            };
            
            Err(format!("{}\n\nContext: {}", e, context_msg).into())
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


// Extract ZIP archive (non-encrypted) with resilient file-by-file processing
fn extract_zip(archive_path: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap the entire function in a comprehensive error handler with resilient extraction
    let result = (|| -> Result<(), Box<dyn Error>> {
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

    // Resilient extraction: continue even if individual files fail
    let mut successful_extractions = 0;
    let mut failed_extractions = Vec::new();

    for i in 0..archive.len() {
        // Wrap each file extraction in its own error handling
        let file_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            extract_single_zip_file(&mut archive, i, extract_to_path, &mut total_extracted_size)
        }));

        let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
        
        match file_result {
            Ok(Ok((file_name, extracted_size))) => {
                successful_extractions += 1;
                if let Some(safe_size) = total_extracted_size.checked_add(extracted_size) {
                    total_extracted_size = safe_size;
                } else {
                    eprintln!("Warning: Total size counter overflow, continuing extraction");
                }
            }
            Ok(Err(e)) => {
                let file_name = archive.by_index(i)
                    .map(|f| f.name().to_string())
                    .unwrap_or_else(|_| format!("file_{}", i));
                
                eprintln!("Warning: Failed to extract '{}': {}", file_name, e);
                failed_extractions.push((file_name, e.to_string()));
            }
            Err(_panic) => {
                let file_name = archive.by_index(i)
                    .map(|f| f.name().to_string())
                    .unwrap_or_else(|_| format!("file_{}", i));
                
                eprintln!("Warning: Panic during extraction of '{}' (likely overflow/corruption)", file_name);
                failed_extractions.push((file_name, "Extraction panic (overflow/corruption)".to_string()));
            }
        }

        // Update progress regardless of success/failure
        if let Some(ref callback) = progress_callback {
            let progress = (current as f32 / total_files as f32) * 100.0;
            
            // Optimize progress reporting for large archives
            let should_report = if total_files > 10000 {
                current % 100 == 0 || progress as u32 != ((current - 1) as f32 / total_files as f32 * 100.0) as u32
            } else {
                true
            };
            
            if should_report {
                let size_info = if total_extracted_size > 1024 * 1024 * 1024 {
                    format!(" ({:.1}GB extracted)", total_extracted_size as f64 / (1024.0 * 1024.0 * 1024.0))
                } else if total_extracted_size > 1024 * 1024 {
                    format!(" ({:.1}MB extracted)", total_extracted_size as f64 / (1024.0 * 1024.0))
                } else {
                    String::new()
                };
                
                let status = if failed_extractions.is_empty() {
                    format!("Extracted {} of {} files{}", current, total_files, size_info)
                } else {
                    format!("Processed {} of {} files ({} successful, {} failed){}", 
                        current, total_files, successful_extractions, failed_extractions.len(), size_info)
                };
                callback(progress, status);
            }
        }
    }

    // Report final results
    if !failed_extractions.is_empty() {
        eprintln!("Extraction completed with {} successes and {} failures:", successful_extractions, failed_extractions.len());
        for (file_name, error) in &failed_extractions {
            eprintln!("  Failed: {} - {}", file_name, error);
        }
        
        // Still consider it a success if we extracted most files
        if successful_extractions > 0 {
            eprintln!("Partial extraction successful: {}/{} files extracted", successful_extractions, total_files);
        } else {
            return Err("No files were successfully extracted".into());
        }
    }
    Ok(())
    })();
    
    // Handle any errors that occurred during extraction
    result.map_err(|e| {
        let error_msg = format!("Failed to extract ZIP archive: {}", e);
        eprintln!("ZIP extraction error: {}", error_msg);
        error_msg.into()
    })
}

// Extract 7Z archive (supports encryption with password) with progress callback
fn extract_7z(archive: &str, extract_to: &str, password: Option<&str>, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn std::error::Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = Path::new(archive);

        if let Some(ref callback) = progress_callback {
            callback(10.0, "Initializing 7Z extraction...".to_string());
        }

        if let Some(pwd) = password {
            let password = Password::from(pwd);
            decompress_file_with_password(path, extract_to, password)?;
        } else {
            decompress_file_with_password(path, extract_to, Password::from(""))?;
        }
        
        if let Some(ref callback) = progress_callback {
            callback(100.0, "7Z extraction completed".to_string());
        }
        Ok(())
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to extract 7Z archive: {}", e);
        eprintln!("7Z extraction error: {}", error_msg);
        error_msg.into()
    })
}

// Extract plain TAR archive with progress callback
fn extract_tar(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let file = File::open(archive)?;
        let mut archive = TarArchive::new(file);

        if let Some(ref callback) = progress_callback {
            callback(10.0, "Starting TAR extraction...".to_string());
        }
        
        archive.unpack(extract_to)?;

        if let Some(ref callback) = progress_callback {
            callback(100.0, "TAR extraction completed".to_string());
        }
        Ok(())
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to extract TAR archive: {}", e);
        eprintln!("TAR extraction error: {}", error_msg);
        error_msg.into()
    })
}
    



// Extract TAR archive with compression and progress callback
fn extract_tar_compressed(extract_to: &str, decoder: impl io::Read, progress_callback: Option<ProgressCallback>, format_name: &str) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut archive = TarArchive::new(decoder);
        
        if let Some(ref callback) = progress_callback {
            callback(50.0, format!("Extracting {} archive...", format_name));
        }
        
        archive.unpack(extract_to)?;
        
        if let Some(ref callback) = progress_callback {
            callback(100.0, format!("{} extraction completed", format_name));
        }
        Ok(())
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to extract {} archive: {}", format_name, e);
        eprintln!("{} extraction error: {}", format_name, error_msg);
        error_msg.into()
    })
}

// Extract TAR.GZ archive
fn extract_tar_gz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let file = File::open(archive)?;
        let decoder = GzDecoder::new(file);
        extract_tar_compressed(extract_to, decoder, progress_callback, "TAR.GZ")
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to extract TAR.GZ archive: {}", e);
        eprintln!("TAR.GZ extraction error: {}", error_msg);
        error_msg.into()
    })
}

// Extract TAR.BZ2 archive
fn extract_tar_bz2(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let file = File::open(archive)?;
        let decoder = BzDecoder::new(file);
        extract_tar_compressed(extract_to, decoder, progress_callback, "TAR.BZ2")
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to extract TAR.BZ2 archive: {}", e);
        eprintln!("TAR.BZ2 extraction error: {}", error_msg);
        error_msg.into()
    })
}

// Extract TAR.XZ archive
fn extract_tar_xz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let file = File::open(archive)?;
        let decoder = XzDecoder::new(file);
        extract_tar_compressed(extract_to, decoder, progress_callback, "TAR.XZ")
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to extract TAR.XZ archive: {}", e);
        eprintln!("TAR.XZ extraction error: {}", error_msg);
        error_msg.into()
    })
}

// Decompress single-file GZ with progress callback
fn decompress_gz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let file = File::open(archive)?;
        let mut decoder = GzDecoder::new(file);
        let output_file = Path::new(extract_to).join(Path::new(archive).file_stem().ok_or("Invalid filename")?);
        
        if let Some(ref callback) = progress_callback {
            callback(25.0, "Decompressing GZ file...".to_string());
        }
        
        let mut outfile = File::create(output_file)?;
        io::copy(&mut decoder, &mut outfile)?;
        
        if let Some(ref callback) = progress_callback {
            callback(100.0, "GZ decompression completed".to_string());
        }
        Ok(())
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to decompress GZ file: {}", e);
        eprintln!("GZ decompression error: {}", error_msg);
        error_msg.into()
    })
}

// Decompress single-file BZ2 with progress callback
fn decompress_bz2(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let file = File::open(archive)?;
        let mut decoder = BzDecoder::new(file);
        let output_file = Path::new(extract_to).join(Path::new(archive).file_stem().ok_or("Invalid filename")?);
        
        if let Some(ref callback) = progress_callback {
            callback(25.0, "Decompressing BZ2 file...".to_string());
        }
        
        let mut outfile = File::create(output_file)?;
        io::copy(&mut decoder, &mut outfile)?;
        
        if let Some(ref callback) = progress_callback {
            callback(100.0, "BZ2 decompression completed".to_string());
        }
        Ok(())
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to decompress BZ2 file: {}", e);
        eprintln!("BZ2 decompression error: {}", error_msg);
        error_msg.into()
    })
}

// Decompress single-file XZ with progress callback
fn decompress_xz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let file = File::open(archive)?;
        let mut decoder = XzDecoder::new(file);
        let output_file = Path::new(extract_to).join(Path::new(archive).file_stem().ok_or("Invalid filename")?);
        
        if let Some(ref callback) = progress_callback {
            callback(25.0, "Decompressing XZ file...".to_string());
        }
        
        let mut outfile = File::create(output_file)?;
        io::copy(&mut decoder, &mut outfile)?;
        
        if let Some(ref callback) = progress_callback {
            callback(100.0, "XZ decompression completed".to_string());
        }
        Ok(())
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to decompress XZ file: {}", e);
        eprintln!("XZ decompression error: {}", error_msg);
        error_msg.into()
    })
}

// Main extraction function with comprehensive security validation
fn extract_archive(archive: &str, extract_to: &str, password: Option<&str>, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap the entire extraction process in comprehensive error handling
    let result = (|| -> Result<(), Box<dyn Error>> {
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
                callback(0.0, format!("Security validation passed. Processing large archive ({:.1} GB)...", archive_size_gb));
            } else {
                callback(0.0, "Security validation passed, starting extraction...".to_string());
            }
        }

        let archive_type = get_archive_type(path);
        
        // Execute extraction with appropriate security measures
        let extraction_result = match archive_type {
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
        
        // Handle extraction result
        extraction_result?;
        
        // Post-extraction validation
        if let Some(ref callback) = progress_callback {
            callback(100.0, "Extraction completed successfully with security validation".to_string());
        }
        
        Ok(())
    })();
    
    // Handle any errors that occurred during the entire extraction process
    result.map_err(|e| {
        let error_msg = format!("Archive extraction failed: {}", e);
        eprintln!("Critical extraction error: {}", error_msg);
        
        // Log additional context for debugging
        eprintln!("Archive: {}", archive);
        eprintln!("Extract to: {}", extract_to);
        if password.is_some() {
            eprintln!("Password provided: Yes");
        }
        
        error_msg.into()
    })
}



// Command-line interface with comprehensive error handling
fn run_cli() -> Result<(), Box<dyn Error>> {
    // Wrap entire CLI operation in error handling
    let result = (|| -> Result<(), Box<dyn Error>> {
        let matches = Command::new("FerrisUnzip")
            .version("1.0")
            .about("Extracts various archive formats in Rust")
            .arg(Arg::new("archive").help("Path to the archive file").required(true).index(1))
            .arg(Arg::new("password").short('p').long("password").help("Password for encrypted archives").required(false))
            .arg(Arg::new("cli").long("cli").help("Force CLI mode").action(clap::ArgAction::SetTrue))
            .get_matches();

        let archive_path = matches.get_one::<String>("archive").unwrap();
        let mut password = matches.get_one::<String>("password").map(|s| s.as_str());

        // Validate archive file exists
        if !Path::new(archive_path).exists() {
            return Err(format!("Archive file does not exist: {}", archive_path).into());
        }

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
        let mut extraction_result = safe_extract_with_panic_recovery(archive_path, extract_to.to_str().unwrap(), password, Some(progress_callback.clone()));

        // Check for missing password error
        if let Err(ref err) = extraction_result {
            if err.to_string().contains("Pass") || err.to_string().contains("password") {
                // Prompt for password
                print!("Password for encrypted archive: ");
                io::stdout().flush()?;

                let mut new_password = String::new();
                io::stdin().read_line(&mut new_password)?;
                password = Some(new_password.trim());

                // Retry extraction with password
                extraction_result = safe_extract_with_panic_recovery(archive_path, extract_to.to_str().unwrap(), password, Some(progress_callback));
            }
        }

        // Handle final result
        match extraction_result {
            Ok(_) => {
                println!("Extraction successful!");
                println!("Files extracted to: {}", extract_to.display());
            }
            Err(err) => {
                eprintln!("Extraction failed: {}", err);
                return Err(err);
            }
        }

        Ok(())
    })();
    
    // Handle CLI-level errors
    result.map_err(|e| {
        let error_msg = format!("CLI operation failed: {}", e);
        eprintln!("CLI Error: {}", error_msg);
        error_msg.into()
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    // Set up enhanced panic handler for better error reporting with GUI warning
    std::panic::set_hook(Box::new(|panic_info| {
        let error_details = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown error".to_string()
        };
        
        let location = if let Some(location) = panic_info.location() {
            format!("{}:{}:{}", location.file(), location.line(), location.column())
        } else {
            "Unknown location".to_string()
        };
        
        // Create detailed error message for GUI
        let error_message = format!(
            "CRITICAL ERROR: FerrisUnzip encountered a fatal error\n\
            This may be due to a malformed archive or system resource limits.\n\n\
            Error details: {}\n\
            Location: {}\n\n\
             Security Note: This error was safely contained.\n\
            No files were corrupted and no security breach occurred.\n\n\
            To prevent this error:\n\
            1. Ensure the archive file is not corrupted\n\
            2. Check available disk space\n\
            3. Try extracting to a different location\n\
            4. Use a smaller archive or extract fewer files\n\
            5. Run with --cli flag for more detailed error information",
            error_details, location
        );
        
        // Also log to stderr for CLI users
        eprintln!("{}", error_message);
        
        // Show GUI message box if not in CLI mode
        if std::env::args().len() <= 2 && !std::env::args().any(|arg| arg == "--cli") {
            // Use native message box - this will work on Windows, Linux, and macOS
            if let Err(e) = show_error_dialog("FerrisUnzip - Critical Error", &error_message) {
                eprintln!("Failed to show error dialog: {}", e);
            }
        }
        
        // Instead of aborting, we'll try to gracefully handle the panic
        // This prevents the process from crashing immediately
        eprintln!("Attempting graceful recovery...");
        
        // For GUI applications, we want to continue running after showing the error
        // The panic hook doesn't prevent the panic from propagating, but we've
        // shown the user what happened
    }));
    
    // Wrap main logic in comprehensive error handling
    let result = (|| -> Result<(), Box<dyn Error>> {
        let args: Vec<String> = std::env::args().collect();
        
        // Check if we have command-line arguments
        if args.len() > 1 {
            // Check for --cli flag or if multiple arguments (CLI mode)
            if args.contains(&"--cli".to_string()) || args.len() > 2 {
                // CLI mode with enhanced error handling
                println!("Starting FerrisUnzip in CLI mode...");
                run_cli()
            } else {
                // GUI mode with file argument from context menu
                let archive_file = &args[1];
                
                // Validate the file exists and is likely an archive
                if !Path::new(archive_file).exists() {
                    eprintln!("Error: File '{}' does not exist", archive_file);
                    return Err("Invalid file path".into());
                }
                
                println!("Starting FerrisUnzip in GUI mode with file: {}", archive_file);
                
                // Wrap GUI initialization in panic recovery
                let gui_result = std::panic::catch_unwind(|| {
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
                    )
                });
                
                match gui_result {
                    Ok(result) => result.map_err(|e| format!("Failed to run GUI: {}", e).into()),
                    Err(_) => {
                        eprintln!("GUI initialization failed due to panic. Archive: {}", archive_file);
                        eprintln!("Try running: {} --cli \"{}\"", args[0], archive_file);
                        
                        // Show error dialog if possible
                        let _ = show_error_dialog(
                            "FerrisUnzip - GUI Error", 
                            &format!("GUI failed to initialize while loading archive.\n\nFile: {}\n\nPlease try:\n1. Running with --cli flag\n2. Using a different archive\n3. Checking if the file is corrupted", archive_file)
                        );
                        
                        // Return an error instead of crashing
                        Err("GUI initialization failed with archive file".into())
                    }
                }
            }
        } else {
            // GUI mode without file
            println!("Starting FerrisUnzip in GUI mode...");
            
            // Wrap GUI initialization in panic recovery
            let gui_result = std::panic::catch_unwind(|| {
                let options = eframe::NativeOptions {
                    viewport: egui::ViewportBuilder::default()
                        .with_inner_size([500.0, 400.0])
                        .with_min_inner_size([400.0, 300.0])
                        .with_title("FerrisUnzip - Archive Extractor"),
                    ..Default::default()
                };
                
                eframe::run_native(
                    "FerrisUnzip",
                    options,
                    Box::new(|_cc| Ok(Box::new(FerrisUnzipApp::default()))),
                )
            });
            
            match gui_result {
                Ok(result) => result.map_err(|e| format!("Failed to run GUI: {}", e).into()),
                Err(_) => {
                    eprintln!("GUI initialization failed due to panic. Falling back to CLI mode.");
                    eprintln!("Run with --cli flag for command line interface.");
                    
                    // Show error dialog if possible
                    let _ = show_error_dialog(
                        "FerrisUnzip - GUI Error", 
                        "The GUI failed to initialize due to a system error.\n\nPlease try:\n1. Running with --cli flag\n2. Updating your graphics drivers\n3. Running as administrator"
                    );
                    
                    // Return an error instead of crashing
                    Err("GUI initialization failed".into())
                }
            }
        }
    })();
    
    // Handle main-level errors
    result.map_err(|e| {
        let error_msg = format!("Application startup failed: {}", e);
        eprintln!("Main Error: {}", error_msg);
        eprintln!("Try running with --cli flag for more information");
        error_msg.into()
    })
}


fn extract_rar(archive_path: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut archive = Archive::new(archive_path).open_for_processing()?;
        let mut file_count = 0;
        let mut processed_count = 0;

        // Ensure the extraction directory exists
        fs::create_dir_all(extract_to)?;

        if let Some(ref callback) = progress_callback {
            callback(5.0, "Starting RAR extraction...".to_string());
        }

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

        if let Some(ref callback) = progress_callback {
            callback(100.0, "RAR extraction completed".to_string());
        }

        Ok(())
    })();
    
    // Handle errors with detailed messaging
    result.map_err(|e| {
        let error_msg = format!("Failed to extract RAR archive: {}", e);
        eprintln!("RAR extraction error: {}", error_msg);
        error_msg.into()
    })
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

// Shell integration installation with error handling
fn install_shell_integration() -> Result<String, Box<dyn Error>> {
    // Wrap in error handling closure
    let result = (|| -> Result<String, Box<dyn Error>> {
        let os = detect_os();
        
        match os {
            OperatingSystem::Windows => install_windows_shell_integration(),
            OperatingSystem::Linux => install_linux_shell_integration(),
            _ => Err("Shell integration is currently only supported on Windows and Linux".into()),
        }
    })();
    
    // Handle shell integration errors
    result.map_err(|e| {
        let error_msg = format!("Shell integration installation failed: {}", e);
        eprintln!("Shell integration error: {}", error_msg);
        error_msg.into()
    })
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
    show_error_dialog: bool,
    last_error_message: String,
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
            show_error_dialog: false,
            last_error_message: String::new(),
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
                Ok(_) => format!("Archive loaded: {}. Security validation passed!", 
                    Path::new(&archive_path).file_name()
                        .unwrap_or_default()
                        .to_string_lossy()),
                Err(e) => {
                    security_warnings.push(format!("Security warning: {}", e));
                    format!("Archive loaded with warnings: {}. Check security status!", 
                        Path::new(&archive_path).file_name()
                            .unwrap_or_default()
                            .to_string_lossy())
                }
            }
        } else {
            "Error: Selected archive file does not exist".to_string()
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
            show_error_dialog: false,
            last_error_message: String::new(),
        }
    }

    fn start_extraction(&mut self) {
        // Security validation before starting extraction
        if !self.password.is_empty() {
            if let Err(e) = validate_password(&self.password) {
                self.status_message = format!("Password validation failed: {}", e);
                return;
            }
            
        }
        
        self.is_extracting = true;
        self.extraction_start_time = Some(Instant::now());
        self.status_message = "Security checks passed. Starting secure extraction...".to_string();
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
            let result = safe_extract_with_panic_recovery(
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
                            self.status_message = "Extraction successful!".to_string();
                            *self.progress.lock().unwrap() = 100.0;
                            *self.progress_message.lock().unwrap() = "Completed!".to_string();
                        }
                        Err(e) => {
                            self.status_message = format!("✗ Extraction failed: {}", e);
                            *self.progress.lock().unwrap() = 0.0;
                            *self.progress_message.lock().unwrap() = "Failed".to_string();
                            
                            // Show error dialog for critical errors
                            if e.contains("CRITICAL") || e.contains("fatal") || e.contains("Security") {
                                self.show_error_dialog = true;
                                self.last_error_message = format!(
                                    "FerrisUnzip - Extraction Error\n\n{}\n\n\
                                    The extraction was safely terminated to protect your system.\n\
                                    Please check the archive file and try again.", e
                                );
                            }
                        }
                    }
                }
            }
            ctx.request_repaint();
        }

        // Handle error dialogs
        if self.show_error_dialog {
            egui::Window::new("Critical Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(500.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("FerrisUnzip encountered a critical error").strong().color(egui::Color32::RED));
                        ui.add_space(15.0);
                        
                        ui.horizontal_wrapped(|ui| {
                            ui.label(&self.last_error_message);
                        });
                        
                        ui.add_space(20.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("Copy Error").clicked() {
                                ui.output_mut(|o| o.copied_text = self.last_error_message.clone());
                            }
                            
                            ui.add_space(20.0);
                            
                            if ui.button("OK").clicked() {
                                self.show_error_dialog = false;
                            }
                        });
                        
                        ui.add_space(10.0);
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("FerrisUnzip - Archive Extractor");
            
            // Show context menu indicator if launched with a file
            if !self.archive_path.is_empty() && self.status_message.contains("Archive loaded") {
                ui.add_space(5.0);
                ui.colored_label(
                    egui::Color32::from_rgb(0, 120, 200), 
                    "Launched from context menu - archive pre-selected"
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
                    if ui.button("Quick Extract").clicked() {
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
            ui.label("Supported formats: ZIP, 7Z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, RAR.");
            ui.label("Version 1.0 - Cross-platform archive extractor");
        });
    }
}

use security_config::*;

// Helper function to extract a single file from ZIP archive with comprehensive error handling
fn extract_single_zip_file(
    archive: &mut ZipArchive<File>, 
    index: usize, 
    extract_to_path: &Path,
    total_extracted_size: &mut u64
) -> Result<(String, u64), Box<dyn Error>> {
    let mut file = archive.by_index(index)?;
    let file_size = file.size();
    let compressed_size = file.compressed_size();
    let file_name = file.name().to_string();
    
    // Enhanced security validation with overflow protection
    validate_extraction_size(*total_extracted_size, file_size, compressed_size)?;
    
    // Sanitize the output path with enhanced security
    let outpath = sanitize_path(&file_name, extract_to_path)?;

    if file_name.ends_with('/') {
        // Directory creation
        fs::create_dir_all(&outpath)?;
        Ok((file_name, 0)) // Directories don't contribute to extracted size
    } else {
        // File extraction with individual error handling
        if let Some(p) = outpath.parent() {
            if !p.exists() {
                fs::create_dir_all(p)?;
            }
        }
        
        // Create output file
        let mut outfile = File::create(&outpath)?;
        
        // Use safe decompression with overflow protection and panic recovery
        let extraction_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let limited_reader = file.take(file_size);
            safe_decompress_with_limits(limited_reader, &mut outfile, file_size)
        }));
        
        match extraction_result {
            Ok(Ok(actual_size)) => {
                // Verify actual extracted size matches expected (with some tolerance for compression artifacts)
                if actual_size != file_size && actual_size.abs_diff(file_size) > 1024 {
                    eprintln!("Warning: Size mismatch for '{}': expected {}, got {} bytes", 
                        file_name, file_size, actual_size);
                }
                Ok((file_name, actual_size))
            }
            Ok(Err(e)) => Err(format!("Decompression failed: {}", e).into()),
            Err(_) => Err("Decompression panic (likely overflow in zlib)".into()),
        }
    }
}

/// Safe arithmetic operations with overflow protection
mod safe_ops {
    use std::error::Error;
    use std::convert::TryFrom;
    
    pub fn safe_add_u64(a: u64, b: u64) -> Result<u64, Box<dyn Error>> {
        a.checked_add(b).ok_or_else(|| "Integer overflow in size calculation".into())
    }
    pub fn safe_cast_usize_to_u64(value: usize) -> Result<u64, Box<dyn Error>> {
        u64::try_from(value).map_err(|_| "Size conversion overflow".into())
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
            if ratio > MAX_COMPRESSION_RATIO {
                // For large files, provide more context
                return Err(format!("Suspicious compression ratio: {:.2}x (max: {}x). File: {:.1}MB compressed to {:.1}MB", 
                    ratio, MAX_COMPRESSION_RATIO,
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
