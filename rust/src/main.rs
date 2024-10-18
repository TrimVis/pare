mod args;
mod db;
mod types;
mod worker;

use crate::args::ARGS;
use crate::types::ResultT;

use fern::Dispatch;
use log::{error, info, warn};
use std::fs::{create_dir_all, File};
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
    assert!(!ARGS.result_db.exists(), "DB file already exists.");
    let out_dir = ARGS.result_db.parent().unwrap().canonicalize().unwrap();
    create_dir_all(out_dir).unwrap();
    let db = db::Db::new()?;
    db.init()?;

    // Runner Setup
    let runner = worker::Runner::new();


    // FIXME: Actually do things

    warn!("This is a warning message.");
    error!("This is an error message.");

    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
