//! RAR archive extraction

use std::error::Error;
use std::fs;
use std::path::Path;
use unrar::Archive;
use crate::{ExtractionConfig, progress::ExtractionProgress};

/// Extract RAR archive
pub fn extract(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let mut archive = Archive::new(archive_path).open_for_processing()?;
    
    // Ensure the extraction directory exists
    fs::create_dir_all(extract_to)?;
    
    // We can't easily count files in RAR archives beforehand, so use indeterminate progress
    let progress = ExtractionProgress::new(0, config.show_progress);
    progress.set_message("Extracting RAR archive...");
    
    let mut file_count = 0u64;
    
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
        
        file_count += 1;
        
        // Update progress with current file
        if let Some(filename) = dest_path.file_name() {
            progress.set_message(&format!("Extracted {} files, current: {}", file_count, filename.to_string_lossy()));
        }
    }
    
    progress.finish();
    Ok(())
}