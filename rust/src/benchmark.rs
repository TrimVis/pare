use rayon::prelude::*;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::args::ARGS;
use crate::gcov::{get_gcov_env, get_prefix, get_prefix_files, process_prefix, symlink_gcno_files};
use crate::utils::{combine_reports, sample_files};

pub fn process_file(
    file: &str,
    batch_id: Option<usize>,
) -> (String, Value) {
    let mut res = format!("| File: {}\n", file);
    let start_time = Instant::now();
    let bid_msg = if let Some(id) = batch_id {
        format!(" in batch {}", id)
    } else {
        String::new()
    };
    let cmd = Path::join(&ARGS.build_dir, "bin/cvc5");

    let output = if ARGS.individual_prefixes {
        let file_path = PathBuf::from(file);
        let env = get_gcov_env(&file_path);
        Command::new(cmd)
            .args([&ARGS.cvc5_args])
            .arg(file)
            .envs(&env)
            .output()
    } else {
        Command::new(cmd).args([&ARGS.cvc5_args]).arg(file).output()
    };

    match output {
        Ok(result) => {
            let sout = String::from_utf8_lossy(&result.stdout);
            res += &sout;
        }
        Err(e) => {
            res += &format!("Error processing file {}: {}\n", file, e);
        }
    };

    let duration = start_time.elapsed().as_millis();
    res += &format!("-> Execution Time: {} ms\n", duration);

    if ARGS.verbose {
        println!(
            "[{}] Execution of /{}/{}{}:\n{}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            ARGS.build_dir.display().to_string(),
            file,
            bid_msg,
            res
        );
    }

    // Processing prefix
    let start_time = Instant::now();
    let prefix = get_prefix(&PathBuf::from(file));
    let files = get_prefix_files(&prefix).unwrap();
    symlink_gcno_files(&prefix).unwrap();
    let files_report = process_prefix(
        &prefix,
        &files,
    )
    .unwrap();

    fs::remove_dir_all(&prefix).unwrap();

    let duration = start_time.elapsed().as_millis();
    res += &format!("-> Processing Time: {} ms\n", duration);

    if ARGS.verbose {
        println!(
            "[{}] Processed prefix for /{}/{}{}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            ARGS.build_dir.display().to_string(),
            file,
            bid_msg
        );
    }

    (res, files_report)
}

pub fn process_file_batch(
    file_batch: Vec<String>,
    batch_id: Option<usize>,
) -> (String, Value) {
    let mut log = String::new();
    let mut report = serde_json::json!({"sources": {}});

    for (i, file) in file_batch.iter().enumerate() {
        let (flog, freport) = process_file(file, batch_id);
        log += &flog;
        combine_reports(&mut report, &freport, false);
        if ARGS.verbose {
            println!(
                "[{}] Combined intermediate results for batch {:?}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                batch_id
            );
        }
        if i % 5 == 4 {
            println!(
                "[{}] Batch {:?}: Processed file {} of {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                batch_id,
                i + 1,
                file_batch.len()
            );
        }
    }

    (log, report)
}

pub fn run_benchmark(
    sample_size: &str,
    bname: &Path,
) -> io::Result<()> {
    let mut report = serde_json::json!({"sources": {}});

    println!(
        "[{}] Retrieving files to be benchmarked... (file count: {})",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        sample_size
    );
    let files = sample_files(sample_size);
    let log_path = bname.with_extension("log");
    let log_file = Arc::new(Mutex::new(File::create(log_path)?));

    writeln!(
        log_file.lock().unwrap(),
        "Running benchmark on {} test files in {}\n",
        sample_size,
        ARGS.benchmark_dir.display().to_string()
    )?;
    writeln!(
        log_file.lock().unwrap(),
        "\n-------------------------------------\n"
    )?;

    println!(
        "[{}] Starting benchmarks...",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    let start_time = Instant::now();

    if ARGS.job_size > 1 {
        let batch_size = std::cmp::max(ARGS.job_size, (files.len() + ARGS.job_size - 1) / ARGS.job_size);
        let file_batches: Vec<Vec<String>> = files
            .chunks(batch_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        println!(
            "[{}] Processing {} batches in {} parallel jobs...",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            file_batches.len(),
            ARGS.job_size
        );

        let report = Arc::new(Mutex::new(report));
        file_batches.par_iter().for_each(|batch| {
            let (log, batch_report) = process_file_batch(
                batch.to_vec(),
                None,
            );
            let mut report_lock = report.lock().unwrap();
            combine_reports(&mut *report_lock, &batch_report, false);
            log_file.lock().unwrap().write_all(log.as_bytes()).unwrap();
        });
    } else {
        for file in files {
            let (log, files_report) =
                process_file(&file, None);
            log_file.lock().unwrap().write_all(log.as_bytes())?;
            combine_reports(&mut report, &files_report, false);
        }
    }

    let duration = start_time.elapsed().as_secs_f64();
    let msg = format!("=> Total Benchmark Runtime: {:.2}s", duration);
    writeln!(log_file.lock().unwrap(), "{}", msg)?;
    println!(
        "\n[{}] {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        msg
    );

    Ok(())
}
