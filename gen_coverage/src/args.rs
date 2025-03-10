use clap::{Parser, Subcommand, ValueEnum};
use mktemp::Temp;
use once_cell::sync::Lazy;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    fs::create_dir,
    path::{Path, PathBuf},
};

use crate::info;

// Global static variable to store parsed CLI arguments
pub static ARGS: Lazy<CliArgs> = Lazy::new(|| {
    let mut args = CliArgs::parse();

    if let Commands::Coverage {
        ref mut tmp_dir, ..
    } = &mut args.command
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

pub static TRACK_UNUSED: Lazy<bool> = Lazy::new(|| {
    if let Commands::Coverage { track_all, .. } = &ARGS.command {
        track_all.unwrap_or(false)
    } else {
        false
    }
});

pub static TRACK_FUNCS: Lazy<bool> = Lazy::new(|| {
    if let Commands::Coverage { coverage_kinds, .. } = &ARGS.command {
        coverage_kinds.contains(&CoverageKind::Functions)
    } else {
        false
    }
});
pub static TRACK_LINES: Lazy<bool> = Lazy::new(|| {
    if let Commands::Coverage { coverage_kinds, .. } = &ARGS.command {
        coverage_kinds.contains(&CoverageKind::Lines)
    } else {
        false
    }
});
pub static TRACK_BRANCHES: Lazy<bool> = Lazy::new(|| {
    if let Commands::Coverage { coverage_kinds, .. } = &ARGS.command {
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
pub static RESULT_TABLE_NAME: Lazy<String> = Lazy::new(|| {
    if let Commands::Evaluate { id } = &ARGS.command {
        let start = SystemTime::now();
        let epoch_time = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        match id {
            Some(v) if v != "" => {
                format!("evaluation_benchmarks_{}_{}", v, epoch_time.as_millis())
            }
            _ => {
                format!("evaluation_benchmarks_{}", epoch_time.as_millis())
            }
        }
    } else {
        "result_benchmarks".to_string()
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

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, name = "Benchmark coverage script")]
pub struct CliArgs {
    /// Repository directory
    #[arg(long = "repo")]
    pub repo_dir: PathBuf,

    /// Number of parallel jobs
    #[arg(short = 'j', long, default_value_t = 1)]
    pub job_size: usize,

    /// Verbose output
    #[arg(short = 'v', long, action = clap::ArgAction::SetTrue)]
    pub verbose: bool,

    /// Additional log file that is being logged to
    #[arg(long, default_value = "./output.log")]
    pub log_file: PathBuf,

    /// Executable (with args) to call
    #[arg(short, long)]
    pub exec: String,

    /// Database which will contain the benchmark results
    pub result_db: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Benchmark coverage script.
    Coverage {
        /// Kinds of code elements for which usage data will be collected
        #[arg(
            short = 'k',
            long,
            // default_value = "functions,branches,lines",
            default_value = "functions",
            value_delimiter = ','
        )]
        coverage_kinds: Vec<CoverageKind>,

        /// Use individual GCOV prefixes for each run
        #[arg(short='p', long="use-prefixes", action = clap::ArgAction::SetTrue)]
        individual_prefixes: bool,

        /// Don't filter out outside libraries from coverage analysis
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_ignore_libs: bool,

        /// Temporary directory where the GCOV outputs are stored
        #[arg(long, default_value = None)]
        tmp_dir: Option<PathBuf>,

        /// Also track unused functions (use carefully, significantly increases DB size)
        #[arg(long, default_value = None)]
        track_all: Option<bool>,

        /// Benchmark file pattern, must contain a path to the benchmark directory,
        /// followed by a pattern e.g. /home/user/benchmarks/non-incremental/**/*.smt2
        #[arg(short, long)]
        benchmarks: String,
    },

    /// Benchmark evaluation script.
    Evaluate {
        #[arg(long, default_value = None)]
        /// ID used in table name to easily identify the result table
        id: Option<String>,
    },
}
