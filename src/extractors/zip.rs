//! ZIP archive extraction with parallel processing

use std::error::Error;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::Arc;
use zip::ZipArchive;
use rayon::prelude::*;
use crate::{ExtractionConfig, progress::ExtractionProgress};

/// Extract ZIP archive with parallel processing
pub fn extract_parallel(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    // Validate that the file exists and is not empty
    let file_metadata = std::fs::metadata(archive_path)?;
    if file_metadata.len() == 0 {
        return Err("Archive file is empty".into());
    }
    
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        format!("Failed to open ZIP archive '{}': {}", archive_path, e)
    })?;
    
    // Create extraction directory
    fs::create_dir_all(extract_to)?;
    
    let total_files = archive.len() as u64;
    let progress = Arc::new(ExtractionProgress::new(total_files, config.show_progress));
    
    // Collect file information for parallel processing
    let mut files_to_extract = Vec::new();
    
    for i in 0..archive.len() {
        let file = match archive.by_index(i) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Warning: Failed to read file at index {}: {}", i, e);
                continue;
            }
        };
        let file_name = file.name().to_string();
        let is_dir = file_name.ends_with('/');
        let compressed_size = file.compressed_size();
        let uncompressed_size = file.size();
        
        files_to_extract.push((i, file_name, is_dir, compressed_size, uncompressed_size));
    }
    
    // Sort by size (largest first) for better load balancing
    files_to_extract.sort_by_key(|&(_, _, _, _, size)| std::cmp::Reverse(size));
    
    // Configure thread pool
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.effective_thread_count())
        .build()?;
    
    // Process files in parallel
    pool.install(|| {
        // First pass: create all directories
        for (_, file_name, is_dir, _, _) in &files_to_extract {
            if *is_dir {
                let outpath = Path::new(extract_to).join(file_name);
                if let Err(e) = fs::create_dir_all(&outpath) {
                    eprintln!("Failed to create directory {}: {}", outpath.display(), e);
                }
                progress.increment();
            }
        }
        
        // Second pass: extract files in parallel
        let non_dir_files: Vec<_> = files_to_extract.into_iter()
            .filter(|(_, _, is_dir, _, _)| !*is_dir)
            .collect();
        
        // We need to handle the archive access more carefully for parallel processing
        // Since ZipArchive is not thread-safe, we'll process in chunks
        let chunk_size = (non_dir_files.len() / config.effective_thread_count()).max(1);
        
        non_dir_files.chunks(chunk_size).collect::<Vec<_>>().into_par_iter().for_each(|chunk| {
            // Each thread gets its own archive instance
            if let Ok(file) = File::open(archive_path) {
                if let Ok(mut thread_archive) = ZipArchive::new(file) {
                    for &(index, ref file_name, _, _, _) in chunk {
                        match extract_single_file(&mut thread_archive, index, file_name, extract_to, config) {
                            Ok(_) => {},
                            Err(e) => {
                                eprintln!("Failed to extract '{}': {}", file_name, e);
                                // Continue with other files instead of panicking
                            }
                        }
                        progress.increment();
                    }
                } else {
                    eprintln!("Failed to open archive for thread processing");
                }
            } else {
                eprintln!("Failed to open file for thread processing");
            }
        });
    });
    
    progress.finish();
    Ok(())
}

/// Extract a single file from the ZIP archive
fn extract_single_file(
    archive: &mut ZipArchive<File>,
    index: usize,
    file_name: &str,
    extract_to: &str,
    config: &ExtractionConfig,
) -> Result<(), Box<dyn Error>> {
    let mut file = archive.by_index(index).map_err(|e| {
        format!("Failed to access file '{}' at index {}: {}", file_name, index, e)
    })?;
    
    let outpath = Path::new(extract_to).join(file_name);
    
    // Ensure parent directories exist
    if let Some(parent) = outpath.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let mut outfile = File::create(&outpath).map_err(|e| {
        format!("Failed to create output file '{}': {}", outpath.display(), e)
    })?;
    
    // Use optimized buffer size based on file size
    let buffer_size = crate::utils::get_optimal_buffer_size(file.size(), config.buffer_size);
    
    // Add size validation to prevent extreme buffer sizes
    let safe_buffer_size = buffer_size.min(1024 * 1024).max(4096); // Cap at 1MB, minimum 4KB
    let mut buffer = vec![0u8; safe_buffer_size];
    
    // Copy with error handling for potential compression issues
    let mut total_read = 0u64;
    let max_size = file.size().saturating_add(1024); // Add some padding for safety
    
    loop {
        let bytes_read = match io::Read::read(&mut file, &mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(e) => {
                return Err(format!("Failed to read from compressed file '{}': {}", file_name, e).into());
            }
        };
        
        total_read = total_read.saturating_add(bytes_read as u64);
        
        // Prevent reading indefinitely if there's a decompression issue
        if total_read > max_size.saturating_mul(10) { // Allow up to 10x expansion
            return Err(format!("File '{}' exceeded expected size during decompression (read {} bytes, expected <= {})", 
                              file_name, total_read, max_size).into());
        }
        
        if let Err(e) = io::Write::write_all(&mut outfile, &buffer[..bytes_read]) {
            return Err(format!("Failed to write to output file '{}': {}", outpath.display(), e).into());
        }
    }
    
    Ok(())
}

/// Fallback extraction method (single-threaded, for compatibility)
pub fn extract_sequential(archive_path: &str, extract_to: &str, config: &ExtractionConfig) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    fs::create_dir_all(extract_to)?;
    
    let total_files = archive.len() as u64;
    let progress = ExtractionProgress::new(total_files, config.show_progress);
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = Path::new(extract_to).join(file.name());
        
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            
            let mut outfile = File::create(&outpath)?;
            let buffer_size = crate::utils::get_optimal_buffer_size(file.size(), config.buffer_size);
            
            // Use optimized copy with larger buffer
            let mut buffer = vec![0u8; buffer_size];
            loop {
                let bytes_read = io::Read::read(&mut file, &mut buffer)?;
                if bytes_read == 0 {
                    break;
                }
                io::Write::write_all(&mut outfile, &buffer[..bytes_read])?;
            }
        }
        
        progress.set_message(&format!("Extracting: {}", file.name()));
        progress.increment();
    }
    
    progress.finish();
    Ok(())
}