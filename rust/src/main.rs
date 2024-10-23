mod args;
mod db;
mod runner;
mod types;
use crate::args::ARGS;
use crate::types::ResultT;

use fern::Dispatch;
pub use log::{error, info, warn};
use std::fs::{create_dir_all, File};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let running = Arc::new(AtomicBool::new(true));

    // SIGINT setup
    let r = running.clone();
    ctrlc::set_handler(move || {
        warn!("Received Ctrl+C! Exiting gracefully...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    // Logger Setup
    Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .chain(std::io::stdout()) // log to console
        .chain(File::create(&ARGS.log_file)?) // log to file
        .apply()?;

    // Db Setup
    if ARGS.result_db.to_str().unwrap() != ":memory:" {
        assert!(!ARGS.result_db.exists(), "DB file already exists.");
        let out_dir = ARGS.result_db.parent().unwrap();
        let out_dir = {
            // Just to make sure we can canonicalize it at all
            if out_dir.is_relative() {
                Path::new("./").join(out_dir).canonicalize().unwrap()
            } else {
                out_dir.canonicalize().unwrap()
            }
        };
        create_dir_all(out_dir).unwrap();
    }

    let mut db = db::Db::new()?;
    db.init()?;

    // Runner Setup
    let mut runner = runner::Runner::new();

    let mut remaining_entries = db.remaining_count()?;
    while remaining_entries > 0 {
        // Early return in case of Ctrl+C
        if !running.load(Ordering::SeqCst) {
            break;
        }

        // We enqueu per iteration in this loop, to ensure that not all gcov results are processed
        // at once
        let gcov_runs = db.retrieve_benchmarks_waiting_for_processing(ARGS.job_size)?;
        for r in gcov_runs {
            runner.enqueue_gcov(r);
        }

        let cvc5_runs = db.retrieve_benchmarks_waiting_for_cvc5(ARGS.job_size)?;
        for r in cvc5_runs {
            runner.enqueue_cvc5(r);
        }

        // TODO: This isn't really optimal with the sleep, but good enough
        thread::sleep(Duration::from_secs(5));
        remaining_entries = db.remaining_count()?;
    }

    // Tell runners to stop after queue has been worked of
    runner.stop();

    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
