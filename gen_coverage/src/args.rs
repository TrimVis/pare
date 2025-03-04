use clap::{Parser, Subcommand, ValueEnum};
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

    if let Some(Commands::Coverage {
        ref mut tmp_dir, ..
    }) = &mut args.command
    {
        // Initialize `tmp_dir` if it hasn't been explicitly provided
        if tmp_dir.is_none() {
            let tmp_base_dir = Path::new("/tmp/coverage_reports");
            if !tmp_base_dir.exists() {
                create_dir(tmp_base_dir).expect("Could not create tmp dir");
            }
            *tmp_dir = Some(Temp::new_dir_in(tmp_base_dir).unwrap().to_path_buf());
            info!(
                "Using temp directory '{}' for intermediate gcov results",
                tmp_dir.as_ref().unwrap().display().to_string()
            )
        }
    }
    args
});

pub static TRACK_FUNCS: Lazy<bool> = Lazy::new(|| {
    if let Some(Commands::Coverage { coverage_kinds, .. }) = &ARGS.command {
        coverage_kinds.contains(&CoverageKind::Functions)
    } else {
        false
    }
});
pub static TRACK_LINES: Lazy<bool> = Lazy::new(|| {
    if let Some(Commands::Coverage { coverage_kinds, .. }) = &ARGS.command {
        coverage_kinds.contains(&CoverageKind::Lines)
    } else {
        false
    }
});
pub static TRACK_BRANCHES: Lazy<bool> = Lazy::new(|| {
    if let Some(Commands::Coverage { coverage_kinds, .. }) = &ARGS.command {
        coverage_kinds.contains(&CoverageKind::Branches)
    } else {
        false
    }
});
pub static EXEC_PLACEHOLDER: Lazy<Vec<String>> = Lazy::new(|| {
    assert!(
        ARGS.exec.contains("{}"),
        "Could not find '{{}}' in exec arg, use this as a placeholder for the benchmark file argument"
    );
    shellwords::split(&ARGS.exec).expect("Could not parse executable command")
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

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, name = "Benchmark coverage script")]
pub struct CliArgs {
    /// Build directory
    #[arg(short, long)]
    pub build_dir: PathBuf,

    /// Number of parallel jobs
    #[arg(short = 'j', long, default_value_t = 1)]
    pub job_size: usize,

    /// Verbose output
    #[arg(short = 'v', long, action = clap::ArgAction::SetTrue)]
    pub verbose: bool,

    /// Additional log file that is being logged to
    #[arg(long, default_value = "./output.log")]
    pub log_file: PathBuf,

    /// Benchmark directory
    pub benchmark_dir: PathBuf,

    /// Database which will contain the benchmark results
    pub result_db: PathBuf,

    /// Executable (with args) to call
    pub exec: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Benchmark coverage script.
    Coverage {
        /// Kinds of code elements for which usage data will be collected
        #[arg(
            short = 'k',
            long,
            default_value = "functions,branches,lines",
            value_delimiter = ','
        )]
        coverage_kinds: Vec<CoverageKind>,

        /// Use individual GCOV prefixes for each run
        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        individual_prefixes: bool,

        /// Don't filter out outside libraries from coverage analysis
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_ignore_libs: bool,

        // Temporary directory where the GCOV outputs are stored
        #[arg(long, default_value = None)]
        tmp_dir: Option<PathBuf>,
    },

    /// Benchmark evaluation script.
    Evaluation {},
}
