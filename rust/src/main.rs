mod args;
mod db;
mod multiwriter;
mod runner;
mod types;
use crate::args::ARGS;
use crate::types::ResultT;

use dur::Duration as DurDuration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
use log::LevelFilter;
pub use log::{error, info, warn};
use multiwriter::MultiWriter;
use std::fs::{remove_dir_all, File};
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
        warn!("Received Ctrl+C! Killing workers...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    let log_target = Box::new(MultiWriter::new(
        std::io::stdout(),
        File::create(&ARGS.log_file).expect("Can't create file"),
    ));

    // Logger Setup
    let logger = env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(log_target))
        // .target(env_logger::Target::Stdout)
        .filter(None, LevelFilter::Info) // Set default log level to Info
        .format_level(true)
        .format_timestamp_secs()
        .build();
    let level = logger.filter();
    let multi = MultiProgress::new();
    // Make sure progress bar and logger don't come into anothers way
    LogWrapper::new(multi.clone(), logger).try_init()?;
    log::set_max_level(level);

    // Runner Setup
    let mut runner = runner::Runner::new();
    runner.wait_on_db_ready();

    let mut db = db::DbReader::new()?;

    // Fancy overall progress bar
    let total_entries = db.remaining_count()?;
    let pb = multi.add(ProgressBar::new(total_entries));
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{msg}] [{wide_bar:40.cyan/blue}] {percent_precise}% {pos:>3}/{len:3} Processing Benchmarks",
            )
            .unwrap()
            .progress_chars("##-"),
    );

    let mut remaining_entries = db.remaining_count()?;

    let loop_start = Instant::now();

    while remaining_entries > 0 {
        // Early return in case of Ctrl+C
        if !running.load(Ordering::SeqCst) {
            runner.stop();
            break;
        }

        let benchmarks = db.retrieve_benchmarks_waiting(10 * ARGS.job_size)?;
        for r in benchmarks {
            runner.enqueue(r);
        }

        thread::sleep(Duration::from_secs(2));
        remaining_entries = db.remaining_count()?;
        let processed_entries = total_entries - remaining_entries;
        let avg_entry_dur = loop_start.elapsed() / (processed_entries + 1).try_into().unwrap();
        let eta = avg_entry_dur * remaining_entries.try_into().unwrap();

        let msg = format!("ETA: {}", DurDuration::from(eta));
        pb.set_message(msg);
        pb.set_position(processed_entries);
    }

    // Wait for runners to work of the queue
    runner.join();

    // Remove the tmp directory
    remove_dir_all(ARGS.tmp_dir.clone().unwrap())?;

    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
