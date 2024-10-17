use clap::Parser;
use once_cell::sync::Lazy;
use std::path::PathBuf;

/// Benchmark coverage script.
#[derive(Parser, Debug)]
#[command(name = "Benchmark coverage script")]
pub struct CliArgs {
    /// Build directory
    #[arg(short, long)]
    pub build_dir: PathBuf,

    /// Arguments for cvc5
    #[arg(short = 'a', long)]
    pub cvc5_args: String,

    /// Sample size ("all", or comma-separated values)
    #[arg(short = 'n', long, default_value = "all", value_delimiter = ',')]
    pub sample_size: Vec<String>,

    /// Use individual GCOV prefixes for each run
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    pub individual: bool,

    /// Number of runs
    #[arg(short = 'r', long, default_value_t = 1)]
    pub no_runs: usize,

    /// Start index for runs
    #[arg(short = 's', long, default_value_t = 1)]
    pub run_start_no: usize,

    /// Number of parallel jobs
    #[arg(short = 'j', long, default_value_t = 1)]
    pub job_size: usize,

    /// Generate lcov as well as fastcov report
    #[arg(short = 'f', long, action = clap::ArgAction::SetTrue)]
    pub full_report: bool,

    /// Verbose output
    #[arg(short = 'v', long, action = clap::ArgAction::SetTrue)]
    pub verbose: bool,

    /// Additional log file that is being logged to
    #[arg(long, default_value = "./output.log")]
    pub log_file: PathBuf,

    /// Benchmark directory
    pub benchmark_dir: PathBuf,

    /// Output directory
    pub output_dir: PathBuf,
}

// Global static variable to store parsed CLI arguments
pub static ARGS: Lazy<CliArgs> = Lazy::new(|| CliArgs::parse());
