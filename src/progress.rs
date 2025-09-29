//! Progress reporting for extraction operations

use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Duration;

/// Progress tracker for extraction operations
pub struct ExtractionProgress {
    pub progress_bar: Option<ProgressBar>,
    pub total_files: u64,
    pub processed_files: Arc<std::sync::atomic::AtomicU64>,
}

impl ExtractionProgress {
    /// Create a new progress tracker
    pub fn new(total_files: u64, show_progress: bool) -> Self {
        let progress_bar = if show_progress && total_files > 0 {
            let pb = ProgressBar::new(total_files);
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} files ({percent}%) {msg}"
                )
                .unwrap()
                .progress_chars("#>-")
            );
            pb.enable_steady_tick(Duration::from_millis(100));
            Some(pb)
        } else {
            None
        };

        Self {
            progress_bar,
            total_files,
            processed_files: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Increment the progress counter
    pub fn increment(&self) {
        let current = self.processed_files.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        
        if let Some(ref pb) = self.progress_bar {
            pb.set_position(current);
        }
    }

    /// Update the progress message
    pub fn set_message(&self, msg: &str) {
        if let Some(ref pb) = self.progress_bar {
            pb.set_message(msg.to_string());
        }
    }

    /// Finish the progress bar
    pub fn finish(&self) {
        if let Some(ref pb) = self.progress_bar {
            pb.finish_with_message("Extraction completed!");
        }
    }

    /// Finish with error message
    pub fn finish_with_error(&self, error: &str) {
        if let Some(ref pb) = self.progress_bar {
            pb.finish_with_message(format!("Error: {}", error));
        }
    }
}