// Hide console window on Windows when running in GUI mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::{Arg, Command};
use std::error::Error;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use zip::ZipArchive;
use sevenz_rust::{decompress_file_with_password, Password};
use tar::Archive as TarArchive;
use flate2::read::GzDecoder;
use bzip2::read::BzDecoder;
use liblzma::read::XzDecoder;
use unrar::Archive;
use std::io::Write;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};

slint::include_modules!();

// Progress callback type
type ProgressCallback = Arc<dyn Fn(f32, String) + Send + Sync>;

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


// Extract ZIP archive (non-encrypted) with progress tracking
fn extract_zip(archive_path: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    let total_files = archive.len();
    let processed = Arc::new(AtomicUsize::new(0));

    // For smaller archives, process sequentially to avoid overhead
    if total_files < 50 {
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = Path::new(extract_to).join(file.name());

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }

            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(ref callback) = progress_callback {
                let progress = (current as f32 / total_files as f32) * 100.0;
                callback(progress, format!("Extracted {} of {} files", current, total_files));
            }
        }
    } else {
        // For larger archives, use parallel processing
        let (tx, rx) = mpsc::channel();
        let extract_to_str = extract_to.to_string();
        let archive_path_str = archive_path.to_string();
        
        // Collect file information first
        let mut file_info = Vec::new();
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name().to_string();
            let is_dir = name.ends_with('/');
            let size = file.size();
            file_info.push((i, name, is_dir, size));
        }
        
        // Process files in parallel batches
        let chunk_size = std::cmp::max(1, total_files / 4);
        let chunks: Vec<_> = file_info.chunks(chunk_size).collect();
        
        for chunk in chunks {
            let tx = tx.clone();
            let extract_to = extract_to_str.clone();
            let archive_path = archive_path_str.clone();
            let chunk = chunk.to_vec();
            
            thread::spawn(move || {
                let file = File::open(&archive_path).unwrap();
                let mut archive = ZipArchive::new(file).unwrap();
                
                for (i, name, is_dir, _size) in chunk {
                    let result: Result<(), Box<dyn Error + Send + Sync>> = (|| {
                        let mut file = archive.by_index(i)?;
                        let outpath = Path::new(&extract_to).join(&name);

                        if is_dir {
                            fs::create_dir_all(&outpath)?;
                        } else {
                            if let Some(p) = outpath.parent() {
                                if !p.exists() {
                                    fs::create_dir_all(p)?;
                                }
                            }
                            let mut outfile = File::create(&outpath)?;
                            io::copy(&mut file, &mut outfile)?;
                        }
                        Ok(())
                    })();
                    
                    tx.send((result, name)).unwrap();
                }
            });
        }
        drop(tx);
        
        // Collect results and update progress
        let mut completed = 0;
        while let Ok((result, filename)) = rx.recv() {
            if let Err(e) = result {
                return Err(format!("Failed to extract {}: {}", filename, e).into());
            }
            
            completed += 1;
            if let Some(ref callback) = progress_callback {
                let progress = (completed as f32 / total_files as f32) * 100.0;
                callback(progress, format!("Extracted {} of {} files", completed, total_files));
            }
        }
    }
    Ok(())
}

// Extract 7Z archive (supports encryption with password) with progress callback
fn extract_7z(archive: &str, extract_to: &str, password: Option<&str>, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn std::error::Error>> {
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
}

// Extract plain TAR archive with progress callback
fn extract_tar(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
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
}


// Extract TAR archive with compression and progress callback
fn extract_tar_compressed(extract_to: &str, decoder: impl io::Read, progress_callback: Option<ProgressCallback>, format_name: &str) -> Result<(), Box<dyn Error>> {
    let mut archive = TarArchive::new(decoder);
    
    if let Some(ref callback) = progress_callback {
        callback(50.0, format!("Extracting {} archive...", format_name));
    }
    
    archive.unpack(extract_to)?;
    
    if let Some(ref callback) = progress_callback {
        callback(100.0, format!("{} extraction completed", format_name));
    }
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
}

// Decompress single-file BZ2 with progress callback
fn decompress_bz2(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
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
}

// Decompress single-file XZ with progress callback
fn decompress_xz(archive: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
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
}

// Main extraction function with progress callback
fn extract_archive(archive: &str, extract_to: &str, password: Option<&str>, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
    let path = Path::new(archive);
    if !path.exists() {
        return Err("Archive file does not exist".into());
    }

    if let Some(ref callback) = progress_callback {
        callback(0.0, "Starting extraction...".to_string());
    }

    let archive_type = get_archive_type(path);
    match archive_type {
        ArchiveType::Zip => extract_zip(archive, extract_to, progress_callback),
        ArchiveType::SevenZ => extract_7z(archive, extract_to, password, progress_callback),
        ArchiveType::Tar => extract_tar(archive, extract_to, progress_callback),
        ArchiveType::TarGz => extract_tar_gz(archive, extract_to, progress_callback),
        ArchiveType::TarBz2 => extract_tar_bz2(archive, extract_to, progress_callback),
        ArchiveType::TarXz => extract_tar_xz(archive, extract_to, progress_callback),
        ArchiveType::Gz => decompress_gz(archive, extract_to, progress_callback),
        ArchiveType::Bz2 => decompress_bz2(archive, extract_to, progress_callback),
        ArchiveType::Xz => decompress_xz(archive, extract_to, progress_callback),
        ArchiveType::Rar => extract_rar(archive, extract_to, progress_callback),
        ArchiveType::Unknown => Err("Unsupported archive format".into()),
    }
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
            
            run_gui_with_file(archive_file.clone())
        }
    } else {
        // GUI mode without file
        run_gui(None)
    }
}

fn run_gui(archive_file: Option<String>) -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;
    
    // State management
    let extraction_result: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));
    let progress = Arc::new(Mutex::new(0.0f32));
    let progress_message = Arc::new(Mutex::new(String::new()));
    
    // If launched with a file, set it up
    if let Some(archive_path) = archive_file {
        ui.set_archive_path(archive_path.clone().into());
        
        // Auto-set extraction path based on archive location
        if let Some(archive_path_obj) = Path::new(&archive_path).parent() {
            if let Some(filename) = Path::new(&archive_path).file_stem() {
                let extract_to = archive_path_obj
                    .join(filename)
                    .display()
                    .to_string();
                ui.set_extract_to_path(extract_to.into());
            }
        }
        
        let status = if Path::new(&archive_path).exists() {
            format!("Archive loaded: {}. Ready to extract!", 
                Path::new(&archive_path).file_name()
                    .unwrap_or_default()
                    .to_string_lossy())
        } else {
            "Error: Selected archive file does not exist".to_string()
        };
        ui.set_status_message(status.into());
        ui.set_show_context_menu_indicator(true);
    }
    
    // Browse archive button
    let ui_weak = ui.as_weak();
    ui.on_browse_archive(move || {
        if let Some(ui) = ui_weak.upgrade() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Archives", &["zip", "7z", "tar", "gz", "bz2", "xz", "rar"])
                .add_filter("All files", &["*"])
                .pick_file()
            {
                let archive_path = path.display().to_string();
                ui.set_archive_path(archive_path.clone().into());
                
                // Auto-set extraction path based on archive location
                if let Some(archive_path_obj) = Path::new(&archive_path).parent() {
                    if let Some(filename) = Path::new(&archive_path).file_stem() {
                        let extract_to = archive_path_obj
                            .join(filename)
                            .display()
                            .to_string();
                        ui.set_extract_to_path(extract_to.into());
                    }
                }
                
                ui.set_status_message("Archive selected. Choose extraction destination.".into());
                ui.set_status_success(false);
                ui.set_status_error(false);
                ui.set_status_extracting(false);
            }
        }
    });
    
    // Browse extract-to button
    let ui_weak = ui.as_weak();
    ui.on_browse_extract_to(move || {
        if let Some(ui) = ui_weak.upgrade() {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                ui.set_extract_to_path(path.display().to_string().into());
            }
        }
    });
    
    // Extract button
    let ui_weak = ui.as_weak();
    let extraction_result_clone = Arc::clone(&extraction_result);
    let progress_clone = Arc::clone(&progress);
    let progress_message_clone = Arc::clone(&progress_message);
    
    ui.on_extract_archive(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let archive_path = ui.get_archive_path().to_string();
            let extract_to = ui.get_extract_to_path().to_string();
            let password_str = ui.get_password().to_string();
            let password = if password_str.is_empty() {
                None
            } else {
                Some(password_str)
            };
            
            ui.set_is_extracting(true);
            ui.set_status_message("Starting extraction...".into());
            ui.set_status_success(false);
            ui.set_status_error(false);
            ui.set_status_extracting(true);
            *progress_clone.lock().unwrap() = 0.0;
            *progress_message_clone.lock().unwrap() = "Preparing...".to_string();
            
            let result_arc = Arc::clone(&extraction_result_clone);
            let progress_arc = Arc::clone(&progress_clone);
            let progress_msg_arc = Arc::clone(&progress_message_clone);
            let ui_weak_inner = ui.as_weak();
            
            // Create progress callback
            let progress_callback: ProgressCallback = Arc::new(move |prog, message| {
                *progress_arc.lock().unwrap() = prog;
                *progress_msg_arc.lock().unwrap() = message.clone();
                
                // Update UI
                if let Some(ui_inner) = ui_weak_inner.upgrade() {
                    ui_inner.set_progress(prog);
                    ui_inner.set_progress_message(message.into());
                }
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
    });
    
    // Install context menu button
    let ui_weak = ui.as_weak();
    ui.on_install_context_menu(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let message = match install_shell_integration() {
                Ok(msg) => {
                    ui.set_install_success(true);
                    format!("✓ {}", msg)
                },
                Err(e) => {
                    ui.set_install_success(false);
                    format!("✗ Installation failed: {}", e)
                },
            };
            ui.set_install_message(message.into());
        }
    });
    
    // Check for extraction completion periodically
    let ui_weak = ui.as_weak();
    let extraction_result_timer = Arc::clone(&extraction_result);
    let progress_timer = Arc::clone(&progress);
    let progress_message_timer = Arc::clone(&progress_message);
    
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                if ui.get_is_extracting() {
                    // Update progress UI
                    let prog = *progress_timer.lock().unwrap();
                    let prog_msg = progress_message_timer.lock().unwrap().clone();
                    ui.set_progress(prog);
                    ui.set_progress_message(prog_msg.into());

                    // Check if extraction is complete
                    if let Ok(mut result) = extraction_result_timer.try_lock() {
                        if let Some(res) = result.take() {
                            ui.set_is_extracting(false);
                            ui.set_status_extracting(false);
                            match res {
                                Ok(_) => {
                                    ui.set_status_message("✓ Extraction successful!".into());
                                    ui.set_status_success(true);
                                    ui.set_status_error(false);
                                    ui.set_progress(100.0);
                                    ui.set_progress_message("Completed!".into());
                                    // Keep completed state visible
                                }
                                Err(e) => {
                                    ui.set_status_message(format!("✗ Extraction failed: {}", e).into());
                                    ui.set_status_success(false);
                                    ui.set_status_error(true);
                                    ui.set_progress(0.0);
                                    ui.set_progress_message("Failed".into());
                                    // Keep failed state visible
                                }
                            }
                        }
                    }
                } else {
                   return;
                }
            }
        },
    );
    
    ui.run()?;
    Ok(())
}

fn run_gui_with_file(archive_file: String) -> Result<(), Box<dyn Error>> {
    run_gui(Some(archive_file))
}


fn extract_rar(archive_path: &str, extract_to: &str, progress_callback: Option<ProgressCallback>) -> Result<(), Box<dyn Error>> {
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
    if std::process::Command::new("update-desktop-database")
        .arg(desktop_entry_dir)
        .output()
        .is_ok() {
        // Command executed successfully
    }
    
    Ok("Shell integration installed successfully! Archive files should now show FerrisUnzip as an option.".to_string())
}

#[cfg(not(target_os = "linux"))]
fn install_linux_shell_integration() -> Result<String, Box<dyn Error>> {
    Err("Linux shell integration is only available on Linux".into())
}

