mod args;
mod benchmark;
mod fastcov;
mod gcov;
mod utils;

use crate::args::ARGS;
use std::time::Instant;

use fern::Dispatch;
use log::{error, info, warn};
use std::fs;
use std::fs::File;
use std::path::Path;

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

    let start = Instant::now();

    gcov::init()?;

    let out_dir = ARGS.output_dir.canonicalize().unwrap();
    let out_dir = Path::new(&out_dir);
    fs::create_dir_all(out_dir).unwrap();

    for sample_size in &ARGS.sample_size {
        for run_number in ARGS.run_start_no..=ARGS.no_runs {
            let bname = &out_dir.join(format!("s{}_{}", sample_size, run_number));

            println!(
                "[{}] Sample Size: {} \tArgs: {} \trun: {}/{}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                sample_size,
                ARGS.cvc5_args,
                run_number,
                ARGS.no_runs
            );

            // Call to run_benchmark function
            benchmark::run_benchmark(sample_size, &bname)?;
        }
    }

    gcov::cleanup()?;

    warn!("This is a warning message.");
    error!("This is an error message.");

    let duration = start.elapsed();
    info!("Total time taken: {} milliseconds", duration.as_millis());

    Ok(())
}
