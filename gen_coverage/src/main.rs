mod args;
mod db;
mod multiwriter;
mod runner;
mod types;
use crate::args::ARGS;
use crate::types::ResultT;
use std::cmp::min;

use dur::Duration as DurDuration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
pub use log::{error, info, warn};
use multiwriter::MultiWriter;
use std::fs::{remove_dir_all, File};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use types::Benchmark;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting benchmark suite");

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
        .format_level(true)
        .format_timestamp_secs()
        .build();
    let level = logger.filter();
    let multi = MultiProgress::new();
    // Make sure progress bar and logger don't come into anothers way
    LogWrapper::new(multi.clone(), logger).try_init()?;
    log::set_max_level(level);

    // Runner Setup
    info!("Creating runners and waiting on db to be initialized");
    let mut runner = runner::Runner::new();
    runner.wait_on_db_ready();
    let benchmarks: Vec<Benchmark> = runner.wait_for_all_benchmarks();

    // Fancy overall progress bar
    let total_count = benchmarks.len();
    let done_pb = multi.add(ProgressBar::new(total_count as u64));
    done_pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{msg}] [{wide_bar:40.cyan/blue}] {percent_precise}% {pos:>3}/{len:3} Processing Benchmarks",
            )
            .unwrap()
            .progress_chars("##-"),
    );

    info!("Enqueuing all benchmarks");
    for b in benchmarks {
        runner.enqueue(b);
    }
    // Ensure that workers terminate after all things have been processed
    runner.enqueue_worker_stop();

    let mut done_count = 0;
    done_pb.reset_elapsed();
    let loop_start = Instant::now();
    while done_count < total_count {
        let eta_msg = if done_count > 0 {
            let avg_entry_dur = loop_start.elapsed() / done_count.try_into().unwrap();
            let eta = avg_entry_dur * (total_count - done_count).try_into().unwrap();
            format!("ETA: {}", DurDuration::from(eta))
        } else {
            "ETA: ?".to_string()
        };
        done_pb.set_message(eta_msg.clone());
        done_pb.set_position(done_count as u64);
        if done_count % min(ARGS.job_size, 100) == 0 {
            info!(" Processed {}/{} Benchmark Files", done_count, total_count);
            info!(" {}", eta_msg);
        }

        done_count += runner.wait_for_next_bench_done() as usize;
        // Early return in case of Ctrl+C or in case we already completed all tasks
        if !running.load(Ordering::SeqCst) || done_count == total_count {
            break;
        }
    }

    done_pb.finish_with_message("Processed all files");

    info!("Gracefully terminating all workers");

    // Wait for runners to work of the queue
    runner.join();

    info!("Deleting the tmp_dir");
    // Remove the tmp directory
    remove_dir_all(ARGS.tmp_dir.clone().unwrap())?;

    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
