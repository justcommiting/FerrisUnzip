use clap::{Arg, Command};
use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use ferris_unzip::{extract_archive_with_config, ExtractionConfig};

fn main() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("FerrisUnzip")
        .version("2.0")
        .about("Fast, parallel archive extraction tool")
        .long_about("FerrisUnzip is a high-performance archive extraction tool that supports multiple formats \
                     with configurable parallel processing for maximum speed.")
        .arg(Arg::new("archive")
            .help("Path to the archive file")
            .required(true)
            .index(1))
        .arg(Arg::new("output")
            .short('o')
            .long("output")
            .help("Output directory for extraction")
            .value_name("DIR"))
        .arg(Arg::new("password")
            .short('p')
            .long("password")
            .help("Password for encrypted archives")
            .value_name("PASSWORD"))
        .arg(Arg::new("threads")
            .short('j')
            .long("threads")
            .help("Number of threads to use for parallel extraction")
            .value_name("COUNT")
            .value_parser(clap::value_parser!(usize)))
        .arg(Arg::new("buffer-size")
            .short('b')
            .long("buffer-size")
            .help("Buffer size in KB for I/O operations (default: 64)")
            .value_name("SIZE_KB")
            .value_parser(clap::value_parser!(usize)))
        .arg(Arg::new("no-progress")
            .long("no-progress")
            .help("Disable progress bar")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("no-mmap")
            .long("no-mmap")
            .help("Disable memory-mapped I/O")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("quiet")
            .short('q')
            .long("quiet")
            .help("Suppress all output except errors")
            .action(clap::ArgAction::SetTrue))
        .get_matches();

    let archive_path = matches.get_one::<String>("archive").unwrap();
    let password = matches.get_one::<String>("password").map(|s| s.clone());
    let quiet = matches.get_flag("quiet");

    // Build configuration from CLI arguments
    let mut config = ExtractionConfig::default();
    
    if let Some(&thread_count) = matches.get_one::<usize>("threads") {
        config.thread_count = thread_count;
    }
    
    if let Some(&buffer_kb) = matches.get_one::<usize>("buffer-size") {
        config.buffer_size = buffer_kb * 1024;
    }
    
    config.show_progress = !matches.get_flag("no-progress") && !quiet;
    config.use_mmap = !matches.get_flag("no-mmap");
    config.password = password;

    // Validate configuration
    if let Err(e) = config.validate() {
        eprintln!("Configuration error: {}", e);
        std::process::exit(1);
    }

    // Determine extraction directory
    let extract_to: PathBuf = if let Some(output_dir) = matches.get_one::<String>("output") {
        PathBuf::from(output_dir)
    } else {
        // Interactive prompt if no output directory specified
        if !quiet {
            print!("Where do you want to extract to? (Leave blank to extract where the file is): ");
            io::stdout().flush()?;
        }

        let mut extract_to_str = String::new();
        io::stdin().read_line(&mut extract_to_str)?;
        let extract_to_str = extract_to_str.trim();

        if !extract_to_str.is_empty() {
            PathBuf::from(extract_to_str)
        } else {
            let archive_path_obj = Path::new(archive_path);
            let archive_dir = archive_path_obj.parent()
                .ok_or("Invalid archive path: Unable to determine parent directory")?;
            let archive_filename = archive_path_obj
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid filename: Unable to extract file stem")?;
            archive_dir.join(archive_filename)
        }
    };

    // Create the extraction directory
    fs::create_dir_all(&extract_to)?;

    if !quiet {
        println!("Extracting {} to {}", archive_path, extract_to.display());
        println!("Using {} threads with {}KB buffer", 
                 config.effective_thread_count(), 
                 config.buffer_size / 1024);
    }

    // Perform extraction
    let result = extract_archive_with_config(
        archive_path, 
        extract_to.to_str().unwrap(), 
        &config
    );

    // Handle password retry for 7Z archives
    if let Err(ref err) = result {
        if err.to_string().contains("password") || err.to_string().contains("Pass") {
            if config.password.is_none() && !quiet {
                print!("Password for encrypted archive: ");
                io::stdout().flush()?;

                let mut new_password = String::new();
                io::stdin().read_line(&mut new_password)?;
                config.password = Some(new_password.trim().to_string());

                // Retry extraction with password
                return match extract_archive_with_config(
                    archive_path, 
                    extract_to.to_str().unwrap(), 
                    &config
                ) {
                    Ok(_) => {
                        if !quiet {
                            println!("Extraction completed successfully!");
                        }
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("Extraction failed: {}", e);
                        std::process::exit(1);
                    }
                };
            }
        }
    }

    // Handle final result
    match result {
        Ok(_) => {
            if !quiet {
                println!("Extraction completed successfully!");
            }
            Ok(())
        }
        Err(err) => {
            eprintln!("Extraction failed: {}", err);
            std::process::exit(1);
        }
    }
}
