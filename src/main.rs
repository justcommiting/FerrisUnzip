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

fn main() -> Result<(), Box<dyn Error>> {
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
