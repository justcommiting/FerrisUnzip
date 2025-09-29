//! Single-file compression format extraction

use std::error::Error;
use std::fs::File;
use std::io;
use std::path::Path;
use flate2::read::GzDecoder;
use bzip2::read::BzDecoder;
use liblzma::read::XzDecoder;
use crate::{ExtractionConfig, progress::ExtractionProgress};

/// Decompress single-file GZ
pub fn decompress_gz(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut decoder = GzDecoder::new(file);
    
    let output_file = Path::new(extract_to).join(
        Path::new(archive_path)
            .file_stem()
            .ok_or("Invalid filename")?
    );
    
    decompress_single_file(&mut decoder, &output_file, "GZ", config)
}

/// Decompress single-file BZ2
pub fn decompress_bz2(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut decoder = BzDecoder::new(file);
    
    let output_file = Path::new(extract_to).join(
        Path::new(archive_path)
            .file_stem()
            .ok_or("Invalid filename")?
    );
    
    decompress_single_file(&mut decoder, &output_file, "BZ2", config)
}

/// Decompress single-file XZ
pub fn decompress_xz(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut decoder = XzDecoder::new(file);
    
    let output_file = Path::new(extract_to).join(
        Path::new(archive_path)
            .file_stem()
            .ok_or("Invalid filename")?
    );
    
    decompress_single_file(&mut decoder, &output_file, "XZ", config)
}

/// Generic function to decompress a single file
fn decompress_single_file<R: io::Read>(
    decoder: &mut R,
    output_path: &Path,
    format_name: &str,
    config: &ExtractionConfig,
) -> Result<(), Box<dyn Error>> {
    let progress = ExtractionProgress::new(1, config.show_progress);
    progress.set_message(&format!("Decompressing {} file...", format_name));
    
    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let mut outfile = File::create(output_path)?;
    
    // Use optimized buffer size
    let mut buffer = vec![0u8; config.buffer_size];
    
    loop {
        let bytes_read = io::Read::read(decoder, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        io::Write::write_all(&mut outfile, &buffer[..bytes_read])?;
    }
    
    progress.increment();
    progress.finish();
    Ok(())
}