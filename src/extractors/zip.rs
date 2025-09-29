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
    
    // For small archives or issues with parallel processing, use sequential extraction
    if archive.len() <= 5 || config.effective_thread_count() == 1 {
        return extract_sequential(archive_path, extract_to, config);
    }
    
    // Try parallel extraction with fallback to sequential on any issues
    match try_parallel_extraction(archive_path, extract_to, config, progress.clone()) {
        Ok(_) => {
            progress.finish();
            Ok(())
        },
        Err(e) => {
            eprintln!("Parallel extraction failed ({}), falling back to sequential", e);
            progress.finish_with_error("Parallel extraction failed, using sequential fallback");
            extract_sequential(archive_path, extract_to, config)
        }
    }
}

/// Attempt parallel extraction with comprehensive error handling
fn try_parallel_extraction(
    archive_path: &str,
    extract_to: &str,
    config: &ExtractionConfig,
    progress: Arc<ExtractionProgress>,
) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    
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
    
    // Configure thread pool with reduced thread count for safety
    let safe_thread_count = config.effective_thread_count().min(8); // Cap at 8 threads to reduce contention
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(safe_thread_count)
        .build()?;
    
    // Use a different approach - catch panics at the thread pool level
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
            
            // Second pass: extract files in parallel with reduced concurrency
            let non_dir_files: Vec<_> = files_to_extract.into_iter()
                .filter(|(_, _, is_dir, _, _)| !*is_dir)
                .collect();
            
            if !non_dir_files.is_empty() {
                // Use smaller chunks to reduce memory pressure and contention
                let chunk_size = (non_dir_files.len() / safe_thread_count).max(1).min(10);
                
                non_dir_files.chunks(chunk_size).collect::<Vec<_>>().into_par_iter().for_each(|chunk| {
                    // Each thread gets its own archive instance
                    let mut extraction_successful = true;
                    
                    if let Ok(file) = File::open(archive_path) {
                        if let Ok(mut thread_archive) = ZipArchive::new(file) {
                            for &(index, ref file_name, _, _, _) in chunk {
                                // Wrap individual extractions in panic catching
                                let extract_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    extract_single_file(&mut thread_archive, index, file_name, extract_to, config)
                                }));
                                
                                match extract_result {
                                    Ok(Ok(_)) => {}, // Success
                                    Ok(Err(e)) => {
                                        eprintln!("Failed to extract '{}': {}", file_name, e);
                                        extraction_successful = false;
                                    },
                                    Err(_) => {
                                        eprintln!("Panic occurred while extracting '{}', skipping", file_name);
                                        extraction_successful = false;
                                    }
                                }
                                progress.increment();
                            }
                        } else {
                            eprintln!("Failed to open archive for thread processing");
                            extraction_successful = false;
                        }
                    } else {
                        eprintln!("Failed to open file for thread processing");
                        extraction_successful = false;
                    }
                    
                    if !extraction_successful {
                        eprintln!("Some files failed to extract in this thread chunk");
                    }
                });
            }
        })
    }));
    
    match result {
        Ok(_) => Ok(()),
        Err(_) => Err("Panic occurred during parallel processing".into()),
    }
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
    
    // Add more conservative size validation to prevent extreme buffer sizes
    let safe_buffer_size = buffer_size.min(256 * 1024).max(4096); // Cap at 256KB, minimum 4KB
    let mut buffer = vec![0u8; safe_buffer_size];
    
    // Copy with error handling for potential compression issues
    let mut total_read = 0u64;
    let uncompressed_size = file.size();
    let max_reasonable_size = if uncompressed_size > 0 {
        uncompressed_size.saturating_mul(2) // Allow up to 2x expansion for safety
    } else {
        10 * 1024 * 1024 // 10MB max for unknown size files
    };
    
    let mut read_iterations = 0u32;
    const MAX_READ_ITERATIONS: u32 = 100_000; // Prevent infinite loops
    
    loop {
        read_iterations += 1;
        if read_iterations > MAX_READ_ITERATIONS {
            return Err(format!("File '{}' exceeded maximum read iterations ({}), possible compression bomb", 
                              file_name, MAX_READ_ITERATIONS).into());
        }
        
        let bytes_read = match io::Read::read(&mut file, &mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(e) => {
                return Err(format!("Failed to read from compressed file '{}' after {} bytes: {}", 
                                 file_name, total_read, e).into());
            }
        };
        
        total_read = total_read.saturating_add(bytes_read as u64);
        
        // Prevent reading too much data if there's a decompression issue
        if total_read > max_reasonable_size {
            return Err(format!("File '{}' exceeded reasonable size during decompression (read {} bytes, expected <= {} bytes)", 
                              file_name, total_read, max_reasonable_size).into());
        }
        
        if let Err(e) = io::Write::write_all(&mut outfile, &buffer[..bytes_read]) {
            return Err(format!("Failed to write to output file '{}': {}", outpath.display(), e).into());
        }
    }
    
    // Validate final size makes sense
    if uncompressed_size > 0 && total_read != uncompressed_size {
        eprintln!("Warning: File '{}' size mismatch - expected {} bytes, got {} bytes", 
                  file_name, uncompressed_size, total_read);
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