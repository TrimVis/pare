use clap::Parser;
use mktemp::Temp;
use once_cell::sync::Lazy;
use std::path::PathBuf;

use crate::info;

// Global static variable to store parsed CLI arguments
pub static ARGS: Lazy<CliArgs> = Lazy::new(|| {
    let mut args = CliArgs::parse();
    // Initialize `tmp_dir` if it hasn't been explicitly provided
    if args.tmp_dir.is_none() {
        args.tmp_dir = Some(Temp::new_dir().unwrap().to_path_buf());
        info!(
            "Using temp directory '{}' for intermediate gcov results",
            args.tmp_dir.as_ref().unwrap().display().to_string()
        )
    }
    args
});

/// Benchmark coverage script.
#[derive(Parser, Debug)]
#[command(name = "Benchmark coverage script")]
pub struct CliArgs {
    /// Build directory
    #[arg(short, long)]
    pub build_dir: PathBuf,

    /// Arguments for cvc5
    #[arg(short = 'a', long, default_value = "--tlimit 2000")]
    pub cvc5_args: String,

    /// Sample size ("all", or comma-separated values)
    #[arg(short = 'n', long, default_value = "all")]
    pub sample_size: String,

    /// Use individual GCOV prefixes for each run
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    pub individual_prefixes: bool,

    /// Number of parallel jobs
    #[arg(short = 'j', long, default_value_t = 1)]
    pub job_size: usize,

    /// Verbose output
    #[arg(short = 'v', long, action = clap::ArgAction::SetTrue)]
    pub verbose: bool,

    /// Don't filter out outside libraries from coverage analysis
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_ignore_libs: bool,

    /// Additional log file that is being logged to
    #[arg(long, default_value = "./output.log")]
    pub log_file: PathBuf,

    // Temporary directory where the GCOV outputs are stored
    #[arg(long, default_value = None)]
    pub tmp_dir: Option<PathBuf>,

    /// Benchmark directory
    pub benchmark_dir: PathBuf,

    /// Database which will contain the benchmark results
    pub result_db: PathBuf,
}
