//! Configuration structures and utilities for FerrisUnzip

use crate::ExtractionConfig;

impl ExtractionConfig {
    /// Create a new configuration with specified thread count
    pub fn with_threads(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    /// Create a new configuration with specified buffer size
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }

    /// Create a new configuration with progress reporting disabled
    pub fn without_progress(mut self) -> Self {
        self.show_progress = false;
        self
    }

    /// Create a new configuration with memory mapping disabled
    pub fn without_mmap(mut self) -> Self {
        self.use_mmap = false;
        self
    }

    /// Set password for encrypted archives
    pub fn with_password(mut self, password: Option<String>) -> Self {
        self.password = password;
        self
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.thread_count == 0 {
            return Err("Thread count must be greater than 0".to_string());
        }

        if self.buffer_size < 1024 {
            return Err("Buffer size must be at least 1KB".to_string());
        }

        if self.buffer_size > 10 * 1024 * 1024 {
            return Err("Buffer size should not exceed 10MB".to_string());
        }

        Ok(())
    }

    /// Get the effective thread count (clamped to reasonable limits)
    pub fn effective_thread_count(&self) -> usize {
        self.thread_count.min(32).max(1) // Cap at 32 threads, minimum 1
    }
}