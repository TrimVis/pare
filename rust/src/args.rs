use clap::{Parser, ValueEnum};
use mktemp::Temp;
use once_cell::sync::Lazy;
use std::fmt;
use std::{
    fs::create_dir,
    path::{Path, PathBuf},
};

use crate::info;

// Global static variable to store parsed CLI arguments
pub static ARGS: Lazy<CliArgs> = Lazy::new(|| {
    let mut args = CliArgs::parse();
    // Initialize `tmp_dir` if it hasn't been explicitly provided
    if args.tmp_dir.is_none() {
        let tmp_base_dir = Path::new("/tmp/coverage_reports");
        if !tmp_base_dir.exists() {
            create_dir(tmp_base_dir).expect("Could not create tmp dir");
        }
        args.tmp_dir = Some(Temp::new_dir_in(tmp_base_dir).unwrap().to_path_buf());
        info!(
            "Using temp directory '{}' for intermediate gcov results",
            args.tmp_dir.as_ref().unwrap().display().to_string()
        )
    }
    args
});

pub static TRACK_FUNCS: Lazy<bool> =
    Lazy::new(|| ARGS.coverage_kinds.contains(&CoverageKind::Functions));
pub static TRACK_LINES: Lazy<bool> =
    Lazy::new(|| ARGS.coverage_kinds.contains(&CoverageKind::Lines));
pub static TRACK_BRANCHES: Lazy<bool> =
    Lazy::new(|| ARGS.coverage_kinds.contains(&CoverageKind::Branches));

pub static DB_USAGE_NAME: Lazy<String> = Lazy::new(|| {
    if ARGS.mode == CoverageMode::Full {
        "execution_count".to_string()
    } else {
        "benchmark_count".to_string()
    }
});

#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum CoverageMode {
    Aggregated,
    Full,
}

#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum CoverageKind {
    Functions,
    Branches,
    Lines,
}

impl fmt::Display for CoverageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CoverageMode::Full => "full",
                CoverageMode::Aggregated => "aggregated",
            }
        )
    }
}

impl fmt::Display for CoverageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CoverageKind::Functions => "functions",
                CoverageKind::Lines => "lines",
                CoverageKind::Branches => "branches",
            }
        )
    }
}

/// Benchmark coverage script.
#[derive(Parser, Debug)]
#[command(name = "Benchmark coverage script")]
pub struct CliArgs {
    /// Build directory
    #[arg(short, long)]
    pub build_dir: PathBuf,

    // TODO: Fix that you can't really pass the argument atm
    /// Arguments for cvc5
    #[arg(short = 'a', long, default_value = "--tlimit 2000")]
    pub cvc5_args: String,

    /// Coverage Mode, determines how much data is stored into the DB
    #[arg(short = 'm', long, default_value_t = CoverageMode::Aggregated)]
    pub mode: CoverageMode,

    /// Kinds of code elements for which usage data will be collected
    #[arg(
        short = 'k',
        long,
        default_value = "functions,branches,lines",
        value_delimiter = ','
    )]
    pub coverage_kinds: Vec<CoverageKind>,

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
