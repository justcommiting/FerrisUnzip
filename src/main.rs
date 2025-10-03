use clap::{Arg, Command};
use eframe::egui;
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
use std::sync::{Arc, Mutex};
use std::thread;

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


// Extract ZIP archive (non-encrypted)
fn extract_zip(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let mut archive = ZipArchive::new(file)?;

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
    }
    Ok(())
}

// Extract 7Z archive (supports encryption with password)
fn extract_7z(archive: &str, extract_to: &str, password: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(archive);

    if let Some(pwd) = password {
        let password = Password::from(pwd); // Convert to Password
        decompress_file_with_password(path, extract_to, password)?;
    } else {
        decompress_file_with_password(path, extract_to, Password::from(""))?; // Empty password for no encryption
    }
    Ok(())
}

// Extract plain TAR archive
fn extract_tar(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let mut archive = TarArchive::new(file); // Explicitly using TarArchive
    archive.unpack(extract_to)?; // No more method not found error
    Ok(())
}


// Extract TAR archive with compression
fn extract_tar_compressed(extract_to: &str, decoder: impl io::Read) -> Result<(), Box<dyn Error>> {
    let mut archive = TarArchive::new(decoder);
    archive.unpack(extract_to)?;
    Ok(())
}

// Extract TAR.GZ archive
fn extract_tar_gz(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let decoder = GzDecoder::new(file);
    extract_tar_compressed(extract_to, decoder)
}

// Extract TAR.BZ2 archive
fn extract_tar_bz2(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let decoder = BzDecoder::new(file);
    extract_tar_compressed(extract_to, decoder)
}

// Extract TAR.XZ archive
fn extract_tar_xz(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let decoder = XzDecoder::new(file);
    extract_tar_compressed(extract_to, decoder)
}

// Decompress single-file GZ
fn decompress_gz(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let mut decoder = GzDecoder::new(file);
    let output_file = Path::new(extract_to).join(Path::new(archive).file_stem().ok_or("Invalid filename")?);
    let mut outfile = File::create(output_file)?;
    io::copy(&mut decoder, &mut outfile)?;
    Ok(())
}

// Decompress single-file BZ2
fn decompress_bz2(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let mut decoder = BzDecoder::new(file);
    let output_file = Path::new(extract_to).join(Path::new(archive).file_stem().ok_or("Invalid filename")?);
    let mut outfile = File::create(output_file)?;
    io::copy(&mut decoder, &mut outfile)?;
    Ok(())
}

// Decompress single-file XZ
fn decompress_xz(archive: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive)?;
    let mut decoder = XzDecoder::new(file);
    let output_file = Path::new(extract_to).join(Path::new(archive).file_stem().ok_or("Invalid filename")?);
    let mut outfile = File::create(output_file)?;
    io::copy(&mut decoder, &mut outfile)?;
    Ok(())
}

// Main extraction function
fn extract_archive(archive: &str, extract_to: &str, password: Option<&str>) -> Result<(), Box<dyn Error>> {
    let path = Path::new(archive);
    if !path.exists() {
        return Err("Archive file does not exist".into());
    }

    let archive_type = get_archive_type(path);
    match archive_type {
        ArchiveType::Zip => extract_zip(archive, extract_to),
        ArchiveType::SevenZ => extract_7z(archive, extract_to, password),
        ArchiveType::Tar => extract_tar(archive, extract_to),
        ArchiveType::TarGz => extract_tar_gz(archive, extract_to),
        ArchiveType::TarBz2 => extract_tar_bz2(archive, extract_to),
        ArchiveType::TarXz => extract_tar_xz(archive, extract_to),
        ArchiveType::Gz => decompress_gz(archive, extract_to),
        ArchiveType::Bz2 => decompress_bz2(archive, extract_to),
        ArchiveType::Xz => decompress_xz(archive, extract_to),
        ArchiveType::Rar => extract_rar(archive, extract_to), // <-- Use extract_rar function here
        ArchiveType::Unknown => Err("Unsupported archive format".into()),
    }
}



// Command-line interface

fn run_cli() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("FerrisUnzip")
        .version("1.0")
        .about("Extracts various archive formats in Rust")
        .arg(Arg::new("archive").help("Path to the archive file").required(true))
        .arg(Arg::new("password").short('p').long("password").help("Password for encrypted 7Z").required(false))
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

    // Attempt extraction
    let mut result = extract_archive(archive_path, extract_to.to_str().unwrap(), password);

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
            result = extract_archive(archive_path, extract_to.to_str().unwrap(), password);
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
    // Check if we have command-line arguments (CLI mode)
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() > 1 {
        // CLI mode
        run_cli()
    } else {
        // GUI mode
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


fn extract_rar(archive_path: &str, extract_to: &str) -> Result<(), Box<dyn Error>> {
    let mut archive = Archive::new(archive_path).open_for_processing()?;

    // Ensure the extraction directory exists
    fs::create_dir_all(extract_to)?;

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
    }

    Ok(())
}

// GUI Application
struct FerrisUnzipApp {
    archive_path: String,
    extract_to_path: String,
    password: String,
    status_message: String,
    is_extracting: bool,
    extraction_result: Arc<Mutex<Option<Result<(), String>>>>,
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
        }
    }
}

impl eframe::App for FerrisUnzipApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for extraction completion
        if self.is_extracting {
            if let Ok(mut result) = self.extraction_result.try_lock() {
                if let Some(res) = result.take() {
                    self.is_extracting = false;
                    match res {
                        Ok(_) => {
                            self.status_message = "✓ Extraction successful!".to_string();
                        }
                        Err(e) => {
                            self.status_message = format!("✗ Extraction failed: {}", e);
                        }
                    }
                }
            }
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("FerrisUnzip - Archive Extractor");
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

            // Extract button
            ui.horizontal(|ui| {
                let can_extract = !self.archive_path.is_empty() 
                    && !self.extract_to_path.is_empty() 
                    && !self.is_extracting;
                
                if ui.add_enabled(can_extract, egui::Button::new("Extract Archive")).clicked() {
                    self.is_extracting = true;
                    self.status_message = "Extracting...".to_string();
                    
                    let archive_path = self.archive_path.clone();
                    let extract_to = self.extract_to_path.clone();
                    let password = if self.password.is_empty() {
                        None
                    } else {
                        Some(self.password.clone())
                    };
                    let result_arc = Arc::clone(&self.extraction_result);
                    
                    // Run extraction in a separate thread
                    thread::spawn(move || {
                        let result = extract_archive(
                            &archive_path,
                            &extract_to,
                            password.as_deref()
                        );
                        
                        let mapped_result = result.map_err(|e| e.to_string());
                        *result_arc.lock().unwrap() = Some(mapped_result);
                    });
                }
                
                if self.is_extracting {
                    ui.spinner();
                }
            });

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

            ui.add_space(15.0);

            // Supported formats
            ui.separator();
            ui.add_space(10.0);
            ui.label("Supported formats: ZIP, 7Z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, RAR");
            ui.label("Version 1.0 - Cross-platform archive extractor");
        });
    }
}
