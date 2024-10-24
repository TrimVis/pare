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
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    panic::set_hook(Box::new(|panic_info| {
        error!("Panic occurred: {:?}", panic_info);
    }));

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
        .filter(None, LevelFilter::Debug) // Set default log level to Info
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
    let total_count = db.total_count()?;
    let done_pb = multi.add(ProgressBar::new(total_count));
    done_pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{msg}] [{wide_bar:40.cyan/blue}] {percent_precise}% {pos:>3}/{len:3} Processing Benchmarks",
            )
            .unwrap()
            .progress_chars("##-"),
    );

    // FIXME: This approach causes Ctrl+C to not work
    // Ensure that there are always enough benchmarks enqueue
    for b in db.retrieve_benchmarks_waiting(None)? {
        runner.enqueue(b);
    }

    let mut done_count = db.waiting_count()?;
    let loop_start = Instant::now();
    while done_count < total_count {
        // Everything below this is only for a nice progress bar and to know when to return
        done_count = db.done_count()?;
        let eta_msg = if done_count > 0 {
            let avg_entry_dur = loop_start.elapsed() / done_count.try_into().unwrap();
            let eta = avg_entry_dur * total_count.try_into().unwrap();
            format!("ETA: {}", DurDuration::from(eta))
        } else {
            "ETA: ?".to_string()
        };
        done_pb.set_message(eta_msg);
        done_pb.set_position(done_count);

        // Early return in case of Ctrl+C or in case we already completed all tasks
        if !running.load(Ordering::SeqCst) || done_count == total_count {
            break;
        }

        // High timeout to not introduce unnecessary overhead
        thread::sleep(Duration::from_secs(60));
    }

    done_pb.finish_with_message("Processed all files");

    info!("Gracefully terminating all workers");
    // Send stop signal (this is safe now, as the queue has been worked of)
    runner.stop();
    // Wait for runners to work of the queue
    runner.join();

    info!("Deleting the tmp_dir");
    // Remove the tmp directory
    remove_dir_all(ARGS.tmp_dir.clone().unwrap())?;

    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
