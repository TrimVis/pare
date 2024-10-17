mod args;
mod benchmark;
mod db;
mod fastcov;
mod gcov;
mod utils;

mod gcov_worker;

use crate::args::ARGS;
use std::time::Instant;

use fern::Dispatch;
use log::{error, info, warn};
use std::fs::{create_dir_all, File};

type ResultT<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logger
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

    assert!(!ARGS.result_db.exists(), "DB file already exists.");

    let start = Instant::now();

    // Ensure directory in which result db will live exists
    let out_dir = ARGS.result_db.parent().unwrap().canonicalize().unwrap();
    create_dir_all(out_dir).unwrap();

    gcov::init()?;

    info!(
        "Sample Size: {} \tArgs: {}",
        ARGS.sample_size, ARGS.cvc5_args,
    );

    // Call to run_benchmark function
    benchmark::run_benchmark(sample_size, &bname)?;

    gcov::cleanup()?;

    warn!("This is a warning message.");
    error!("This is an error message.");

    let start = Instant::now();
    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
