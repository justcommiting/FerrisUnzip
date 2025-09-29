//! TAR archive extraction with compression support

use std::error::Error;
use std::fs::File;
use std::io;
use tar::Archive as TarArchive;
use flate2::read::GzDecoder;
use bzip2::read::BzDecoder;
use liblzma::read::XzDecoder;
use crate::{ExtractionConfig, progress::ExtractionProgress};

/// Extract plain TAR archive
pub fn extract(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let progress = ExtractionProgress::new(0, config.show_progress);
    progress.set_message("Extracting TAR archive...");
    
    let mut archive = TarArchive::new(file);
    
    // Set up archive with larger buffer for better performance
    if config.buffer_size > 64 * 1024 {
        // For larger buffer sizes, we could implement custom buffering
        // but tar crate handles this internally
    }
    
    match archive.unpack(extract_to) {
        Ok(_) => {
            progress.finish();
            Ok(())
        }
        Err(e) => {
            progress.finish_with_error(&e.to_string());
            Err(e.into())
        }
    }
}

/// Extract TAR.GZ archive
pub fn extract_gz(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    extract_compressed(extract_to, decoder, "TAR.GZ", config)
}

/// Extract TAR.BZ2 archive
pub fn extract_bz2(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let decoder = BzDecoder::new(file);
    extract_compressed(extract_to, decoder, "TAR.BZ2", config)
}

/// Extract TAR.XZ archive
pub fn extract_xz(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let decoder = XzDecoder::new(file);
    extract_compressed(extract_to, decoder, "TAR.XZ", config)
}

/// Extract TAR archive with compression (generic function)
fn extract_compressed<R: io::Read>(
    extract_to: &str,
    decoder: R,
    format_name: &str,
    config: &ExtractionConfig,
) -> Result<(), Box<dyn Error>> {
    let progress = ExtractionProgress::new(0, config.show_progress);
    progress.set_message(&format!("Extracting {} archive...", format_name));
    
    let mut archive = TarArchive::new(decoder);
    
    match archive.unpack(extract_to) {
        Ok(_) => {
            progress.finish();
            Ok(())
        }
        Err(e) => {
            progress.finish_with_error(&e.to_string());
            Err(e.into())
        }
    }
}