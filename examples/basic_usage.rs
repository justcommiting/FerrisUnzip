//! Basic usage example for FerrisUnzip library

use ferris_unzip::{extract_archive_with_config, ExtractionConfig};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Example 1: Use default configuration
    let default_config = ExtractionConfig::default();
    println!("Default config: {} threads, {}KB buffer", 
             default_config.thread_count, 
             default_config.buffer_size / 1024);
    
    // Example 2: Custom high-performance configuration
    let fast_config = ExtractionConfig::default()
        .with_threads(8)
        .with_buffer_size(256 * 1024) // 256KB buffer
        .without_progress(); // Disable progress for batch processing
    
    println!("Fast config: {} threads, {}KB buffer", 
             fast_config.thread_count, 
             fast_config.buffer_size / 1024);
    
    // Example 3: Memory-constrained configuration
    let small_config = ExtractionConfig::default()
        .with_threads(2)
        .with_buffer_size(32 * 1024) // 32KB buffer
        .without_mmap(); // Disable memory mapping
    
    println!("Small config: {} threads, {}KB buffer, mmap: {}", 
             small_config.thread_count, 
             small_config.buffer_size / 1024,
             small_config.use_mmap);
    
    // Example 4: Configuration with password
    let secure_config = ExtractionConfig::default()
        .with_password(Some("mysecretpassword".to_string()));
    
    println!("Secure config has password: {}", secure_config.password.is_some());
    
    // To actually extract archives, you would use:
    // extract_archive_with_config("archive.zip", "/extract/to", &config)?;
    
    println!("Example completed successfully!");
    Ok(())
}