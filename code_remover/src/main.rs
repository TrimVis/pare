mod analysis;
mod remover;
mod remover_config;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Visualizes the distribution of function start and end lines, versus what is actually found,
    /// with the current detection method
    VisualizeFunctionRanges {
        #[arg(long)]
        db: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Replaces substring in paths extracted from DB, to accomodate for a system change
        #[arg(long, num_args = 2, value_names=vec!["FROM", "TO"])]
        path_rewrite: Option<Vec<String>>,
    },

    /// Finds the smallest benchmark for each removed function, for further analysis
    RetrieveSmallestBenches {
        #[arg(long)]
        db: PathBuf,

        #[arg(short, long)]
        p: f64,

        // Show the x top tokens used across the smallest benchmarks
        #[arg(short, long)]
        top_tokens: Option<usize>,

        /// Replaces substring in paths extracted from DB, to accomodate for a system change
        #[arg(long, num_args = 2, value_names=vec!["FROM", "TO"])]
        path_rewrite: Option<Vec<String>>,
    },

    /// Retrieves the set of benchmarks that should still be working according optimization result
    RetrieveWorkingBenchmarks {
        #[arg(long)]
        db: PathBuf,

        #[arg(short, long)]
        p: f64,

        /// Replaces substring in paths extracted from DB, to accomodate for a system change
        #[arg(long, num_args = 2, value_names=vec!["FROM", "TO"])]
        path_rewrite: Option<Vec<String>>,
    },

    /// Remove the functions that have been determined as unneccessary by our optimization step
    Remove {
        #[arg(short, long)]
        config: PathBuf,

        #[arg(long)]
        no_change: Option<bool>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Remove { config, no_change }) => {
            let mut remover = remover::Remover::new(config);
            remover.remove(no_change.unwrap_or(true))?;
        }
        Some(Commands::VisualizeFunctionRanges {
            db,
            output,
            path_rewrite,
        }) => {
            let mut analyzer = analysis::Analyzer::new(db.display().to_string(), path_rewrite);
            analyzer.analyze_line_deviations()?;
            analyzer.visualize_line_deviations(
                output
                    .unwrap_or(PathBuf::from("./function_line_deviation.png"))
                    .to_str()
                    .unwrap(),
            )?;
        }
        Some(Commands::RetrieveSmallestBenches {
            db,
            p,
            path_rewrite,
            top_tokens,
        }) => {
            let mut analyzer = analysis::Analyzer::new(db.display().to_string(), path_rewrite);
            analyzer.analyze_smallest_benches(p, top_tokens)?;
        }
        Some(Commands::RetrieveWorkingBenchmarks {
            db,
            p,
            path_rewrite,
        }) => {
            let mut analyzer = analysis::Analyzer::new(db.display().to_string(), path_rewrite);
            analyzer.analyze_working_benches(p)?;
        }
        None => {}
    }

    Ok(())
}
