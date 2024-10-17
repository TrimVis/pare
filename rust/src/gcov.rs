use glob::glob;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Output;

use crate::args::ARGS;
use crate::utils::combine_reports;

// External constants (Replace with the actual constant value)
const GCOV_PREFIX_BASE: &str = "/path/to/gcov_prefix_base";

// Initialize gcov directories and cleanup old data
pub fn init() -> io::Result<()> {
    fs::create_dir_all(GCOV_PREFIX_BASE)?;

    let pattern = format!(
        "{}/**/*.smt2",
        ARGS.benchmark_dir
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(file) = entry {
            let dir_path = get_prefix(&file);
            fs::create_dir_all(&dir_path)?;
        }
    }

    Ok(())
}

// Cleanup gcov data
pub fn cleanup() -> io::Result<()> {
    fs::remove_dir_all(GCOV_PREFIX_BASE)?;
    Ok(())
}

// Symlink `.gcno` files from build directory to prefix directory
pub fn symlink_gcno_files(prefix_dir: &Path) -> io::Result<()> {
    if prefix_dir == Path::new("/") {
        if ARGS.verbose {
            println!("Empty prefix, early return");
        }
        return Ok(());
    }

    let build_dir = fs::canonicalize(&ARGS.build_dir)?;
    let prefix_dir = fs::canonicalize(prefix_dir)?;

    let pattern = format!("{}/**/*.gcno", build_dir.to_string_lossy());
    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(gcno_file) = entry {
            let target_dir = prefix_dir.join(gcno_file.parent().unwrap());
            fs::create_dir_all(&target_dir)?;

            let target_file = prefix_dir.join(&gcno_file);
            if !target_file.exists() {
                symlink(&gcno_file, &target_file)?;
                if ARGS.verbose {
                    println!(
                        "Created symlink: {} -> {}",
                        target_file.display(),
                        gcno_file.display()
                    );
                }
            } else if ARGS.verbose {
                println!("Symlink already exists: {}", target_file.display());
            }
        }
    }
    Ok(())
}

// Get a unique file identifier using SHA-256 and the file path
pub fn get_file_uid(file: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let mut h_readable: String = file
        .components()
        .rev()
        .take(2)
        .map(|comp| comp.as_os_str().to_string_lossy().to_string())
        .collect();

    if h_readable.ends_with(".smt2") {
        h_readable.truncate(h_readable.len() - 5);
    }
    if h_readable.len() > 20 {
        h_readable.truncate(20);
    }

    format!("{}-{}", hash, h_readable)
}

// Get the GCOV_PREFIX for a file
pub fn get_prefix(file: &Path) -> PathBuf {
    Path::new(GCOV_PREFIX_BASE).join(get_file_uid(file))
}

// Prepare environment variables for running gcov
pub fn get_gcov_env(file: &Path) -> HashMap<String, String> {
    let mut env = env::vars().collect::<HashMap<_, _>>();
    env.insert(
        "GCOV_PREFIX".to_string(),
        get_prefix(file).to_string_lossy().to_string(),
    );
    env
}

// Get all gcda files from the given prefix
pub fn get_prefix_files(prefix: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let pattern = format!("{}/**/*.gcda", prefix.canonicalize()?.display());
    let files = glob(&pattern)?
        .filter_map(Result::ok)
        .collect::<Vec<PathBuf>>();

    Ok(files)
}

// Process a prefix by running gcov on the files and combining the results
pub fn process_prefix(prefix: &Path, files: &Vec<PathBuf>) -> io::Result<Value> {
    let mut files_report = HashMap::new();
    files_report.insert("sources".to_string(), serde_json::json!({}));
    let mut files_report_value = Value::Object(files_report.into_iter().collect());

    for gcda_file in files {
        let mut env = env::vars().collect::<HashMap<_, _>>();
        env.insert(
            "GCOV_PREFIX".to_string(),
            prefix.canonicalize()?.display().to_string(),
        );

        if ARGS.verbose {
            println!("Gcov GCDA File: {}", gcda_file.display());
        }

        let output = run_gcov(gcda_file, &env)?;

        if ARGS.verbose {
            println!("Gcov Exit Code: {}", output.status);
            println!("Gcov Errors: {}", String::from_utf8_lossy(&output.stderr));
        }

        let source: Value = serde_json::from_slice(&output.stdout)?;
        let next_report = process_gcov_output(&source)?;
        // FIXME: This should call distillSource

        let next_report_value = Value::Object(next_report.into_iter().collect());

        combine_reports(&mut files_report_value, &next_report_value, true);
    }

    Ok(files_report_value)
}

// Function to run the gcov command on a file
fn run_gcov(gcda_file: &Path, env: &HashMap<String, String>) -> io::Result<Output> {
    Command::new("gcov")
        .args(&["--json", "--stdout", gcda_file.to_str().unwrap()])
        .envs(env)
        .output()
}

// Process the gcov output to distill sources (dummy function)
fn process_gcov_output(source: &Value) -> io::Result<HashMap<String, Value>> {
    // Placeholder for processing gcov output
    Ok(HashMap::new())
}
