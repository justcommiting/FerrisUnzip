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
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    // Create extraction directory
    fs::create_dir_all(extract_to)?;
    
    let total_files = archive.len() as u64;
    let progress = Arc::new(ExtractionProgress::new(total_files, config.show_progress));
    
    // Collect file information for parallel processing
    let mut files_to_extract = Vec::new();
    
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
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
                        if let Err(e) = extract_single_file(&mut thread_archive, index, file_name, extract_to, config) {
                            eprintln!("Failed to extract {}: {}", file_name, e);
                        }
                        progress.increment();
                    }
                }
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
    let mut file = archive.by_index(index)?;
    let outpath = Path::new(extract_to).join(file_name);
    
    // Ensure parent directories exist
    if let Some(parent) = outpath.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let mut outfile = File::create(&outpath)?;
    
    // Use optimized buffer size based on file size
    let buffer_size = crate::utils::get_optimal_buffer_size(file.size(), config.buffer_size);
    
    // Copy with custom buffer size
    let mut buffer = vec![0u8; buffer_size];
    loop {
        let bytes_read = io::Read::read(&mut file, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        io::Write::write_all(&mut outfile, &buffer[..bytes_read])?;
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