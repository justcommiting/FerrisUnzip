//! FerrisUnzip - A fast, parallel archive extraction library
//! 
//! This library provides efficient extraction of various archive formats including:
//! - ZIP archives
//! - 7Z archives (with password support)
//! - TAR archives (plain and compressed with GZ, BZ2, XZ)
//! - Single-file compression formats (GZ, BZ2, XZ)
//! - RAR archives
//! 
//! Features:
//! - Configurable thread pools for parallel extraction
//! - Progress reporting
//! - Memory-mapped I/O for large files
//! - Optimized buffer sizes

pub mod extractors;
pub mod progress;
pub mod config;
pub mod utils;

use std::error::Error;
use std::path::Path;

/// Supported archive types
#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveType {
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

/// Configuration for extraction operations
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Number of threads to use for parallel extraction
    pub thread_count: usize,
    /// Buffer size for I/O operations
    pub buffer_size: usize,
    /// Whether to show progress bars
    pub show_progress: bool,
    /// Whether to use memory-mapped I/O for large files
    pub use_mmap: bool,
    /// Password for encrypted archives
    pub password: Option<String>,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            thread_count: num_cpus::get(),
            buffer_size: 64 * 1024, // 64KB default buffer
            show_progress: true,
            use_mmap: true,
            password: None,
        }
    }
}

/// Main extraction function with configurable options
pub fn extract_archive_with_config(
    archive_path: &str,
    extract_to: &str,
    config: &ExtractionConfig,
) -> Result<(), Box<dyn Error>> {
    // Validate input parameters
    if archive_path.is_empty() {
        return Err("Archive path cannot be empty".into());
    }
    
    if extract_to.is_empty() {
        return Err("Extract path cannot be empty".into());
    }
    
    let path = Path::new(archive_path);
    if !path.exists() {
        return Err(format!("Archive file '{}' does not exist", archive_path).into());
    }
    
    // Check if it's actually a file and not a directory
    if path.is_dir() {
        return Err(format!("'{}' is a directory, not an archive file", archive_path).into());
    }
    
    // Check file size - reject empty files
    let metadata = std::fs::metadata(path)?;
    if metadata.len() == 0 {
        return Err(format!("Archive file '{}' is empty", archive_path).into());
    }

    let archive_type = utils::get_archive_type(path);
    
    match archive_type {
        ArchiveType::Zip => extractors::zip::extract_parallel(archive_path, extract_to, config),
        ArchiveType::SevenZ => extractors::sevenz::extract(archive_path, extract_to, config),
        ArchiveType::Tar => extractors::tar::extract(archive_path, extract_to, config),
        ArchiveType::TarGz => extractors::tar::extract_gz(archive_path, extract_to, config),
        ArchiveType::TarBz2 => extractors::tar::extract_bz2(archive_path, extract_to, config),
        ArchiveType::TarXz => extractors::tar::extract_xz(archive_path, extract_to, config),
        ArchiveType::Gz => extractors::compression::decompress_gz(archive_path, extract_to, config),
        ArchiveType::Bz2 => extractors::compression::decompress_bz2(archive_path, extract_to, config),
        ArchiveType::Xz => extractors::compression::decompress_xz(archive_path, extract_to, config),
        ArchiveType::Rar => extractors::rar::extract(archive_path, extract_to, config),
        ArchiveType::Unknown => Err(format!("Unsupported archive format for file '{}'", archive_path).into()),
    }
}

/// Simple extraction function for backward compatibility
pub fn extract_archive(
    archive_path: &str,
    extract_to: &str,
    password: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let mut config = ExtractionConfig::default();
    config.password = password.map(|s| s.to_string());
    config.show_progress = false; // Disable progress for compatibility
    
    extract_archive_with_config(archive_path, extract_to, &config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_archive_type_detection() {
        let test_cases = vec![
            ("test.zip", ArchiveType::Zip),
            ("test.7z", ArchiveType::SevenZ),
            ("test.tar", ArchiveType::Tar),
            ("test.tar.gz", ArchiveType::TarGz),
            ("test.tar.bz2", ArchiveType::TarBz2),
            ("test.tar.xz", ArchiveType::TarXz),
            ("test.gz", ArchiveType::Gz),
            ("test.bz2", ArchiveType::Bz2),
            ("test.xz", ArchiveType::Xz),
            ("test.rar", ArchiveType::Rar),
            ("test.unknown", ArchiveType::Unknown),
        ];

        for (filename, expected) in test_cases {
            let path = Path::new(filename);
            let result = utils::get_archive_type(path);
            assert_eq!(result, expected, "Failed for {}", filename);
        }
    }

    #[test]
    fn test_extraction_config_defaults() {
        let config = ExtractionConfig::default();
        assert!(config.thread_count > 0);
        assert!(config.buffer_size >= 1024);
        assert!(config.show_progress);
        assert!(config.use_mmap);
        assert!(config.password.is_none());
    }

    #[test]
    fn test_extraction_config_validation() {
        let mut config = ExtractionConfig::default();
        
        // Valid config should pass
        assert!(config.validate().is_ok());
        
        // Invalid thread count
        config.thread_count = 0;
        assert!(config.validate().is_err());
        
        // Invalid buffer size
        config.thread_count = 1;
        config.buffer_size = 512;
        assert!(config.validate().is_err());
        
        // Too large buffer size
        config.buffer_size = 20 * 1024 * 1024;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_buffer_size_optimization() {
        let small_file = 1024; // 1KB
        let medium_file = 50 * 1024 * 1024; // 50MB
        let large_file = 200 * 1024 * 1024; // 200MB
        let default_buffer = 64 * 1024;

        assert_eq!(utils::get_optimal_buffer_size(small_file, default_buffer), default_buffer);
        assert!(utils::get_optimal_buffer_size(medium_file, default_buffer) > default_buffer);
        assert_eq!(utils::get_optimal_buffer_size(large_file, default_buffer), 1024 * 1024);
    }

    #[test]
    fn test_empty_archive_path() {
        let config = ExtractionConfig::default();
        let result = extract_archive_with_config("", "/tmp/test", &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Archive path cannot be empty"));
    }

    #[test]
    fn test_empty_extract_path() {
        let config = ExtractionConfig::default();
        let result = extract_archive_with_config("test.zip", "", &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Extract path cannot be empty"));
    }

    #[test]
    fn test_nonexistent_file() {
        let config = ExtractionConfig::default();
        let result = extract_archive_with_config("nonexistent.zip", "/tmp/test", &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_directory_as_archive() {
        use std::fs;
        let temp_dir = "/tmp/test_dir_archive";
        fs::create_dir_all(temp_dir).unwrap();
        
        let config = ExtractionConfig::default();
        let result = extract_archive_with_config(temp_dir, "/tmp/test", &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("is a directory"));
        
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_empty_file() {
        use std::fs::File;
        let empty_file = "/tmp/empty_test.zip";
        File::create(empty_file).unwrap();
        
        let config = ExtractionConfig::default();
        let result = extract_archive_with_config(empty_file, "/tmp/test", &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("is empty"));
        
        std::fs::remove_file(empty_file).unwrap();
    }
}