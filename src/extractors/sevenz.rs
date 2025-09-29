//! 7Z archive extraction

use std::error::Error;
use std::path::Path;
use sevenz_rust::{decompress_file_with_password, Password};
use crate::{ExtractionConfig, progress::ExtractionProgress};

/// Extract 7Z archive (supports encryption with password)
pub fn extract(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let path = Path::new(archive_path);
    
    // Show indeterminate progress for 7Z (we can't get file count easily)
    let progress = ExtractionProgress::new(0, config.show_progress);
    progress.set_message("Extracting 7Z archive...");
    
    let result = if let Some(ref pwd) = config.password {
        let password = Password::from(pwd.as_str());
        decompress_file_with_password(path, extract_to, password)
    } else {
        decompress_file_with_password(path, extract_to, Password::from(""))
    };
    
    match result {
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