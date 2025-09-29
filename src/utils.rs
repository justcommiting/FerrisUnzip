//! Utility functions for FerrisUnzip

use std::path::Path;
use crate::ArchiveType;

/// Determine archive type based on file extension
pub fn get_archive_type(path: &Path) -> ArchiveType {
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

/// Get optimal buffer size based on file size
pub fn get_optimal_buffer_size(file_size: u64, default_buffer_size: usize) -> usize {
    // Use larger buffers for bigger files, but cap at 1MB
    let optimal = if file_size > 100 * 1024 * 1024 {
        // Files > 100MB: use 1MB buffer
        1024 * 1024
    } else if file_size > 10 * 1024 * 1024 {
        // Files > 10MB: use 256KB buffer
        256 * 1024
    } else {
        // Smaller files: use default buffer
        default_buffer_size
    };
    
    optimal.max(default_buffer_size)
}

/// Check if file should use memory mapping based on size
pub fn should_use_mmap(file_size: u64, use_mmap_config: bool) -> bool {
    use_mmap_config && file_size > 10 * 1024 * 1024 // Use mmap for files > 10MB
}