mod args;
mod db;
mod types;
mod worker;

use crate::args::ARGS;
use crate::types::ResultT;

use fern::Dispatch;
use log::{error, info, warn};
use std::fs::{create_dir_all, File};
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    info!(
        "Sample Size: {} \tArgs: {}",
        ARGS.sample_size, ARGS.cvc5_args,
    );

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
    // FIXME: These checks prevent the use of in-memory DB via ":memory:" as arg
    // assert!(!ARGS.result_db.exists(), "DB file already exists.");
    // let out_dir = ARGS.result_db.parent().unwrap().canonicalize().unwrap();
    // create_dir_all(out_dir).unwrap();

    let mut db = db::Db::new()?;
    db.init()?;

    // return Ok(());

    info!("This is a info message.");
    warn!("This is a warning message.");
    error!("This is an error message.");

    // Runner Setup
    let runner = worker::Runner::new();

    let mut remaining_entries = db.remaining_count()?;
    while remaining_entries > 0 {
        let gcov_runs = db.retrieve_benchmarks_waiting_for_processing(ARGS.job_size * 2)?;
        for r in gcov_runs {
            runner.enqueue_gcov(&mut db, r);
        }

        let cvc5_runs = db.retrieve_benchmarks_waiting_for_cvc5(ARGS.job_size * 2)?;
        for r in cvc5_runs {
            runner.enqueue_cvc5(&mut db, r);
        }

        thread::sleep(Duration::from_secs(5));
        remaining_entries = db.remaining_count()?;
    }

    // FIXME: Actually do things


    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
