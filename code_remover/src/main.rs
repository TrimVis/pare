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
    },

    // Finds the smallest benchmark for each removed function, for further analysis
    RetrieveSmallestBenches {
        #[arg(long)]
        db: PathBuf,

        #[arg(short, long)]
        p: f64,
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
            let remover = remover::Remover::new(config);
            remover.remove(no_change.unwrap_or(true))?;
        }
        Some(Commands::VisualizeFunctionRanges { db, output }) => {
            let mut analyzer = analysis::Analyzer::new(db.display().to_string());
            analyzer.analyze_line_deviations()?;
            analyzer.visualize_line_deviations(
                output
                    .unwrap_or(PathBuf::from("./function_line_deviation.png"))
                    .to_str()
                    .unwrap(),
            )?;
        }
        Some(Commands::RetrieveSmallestBenches { db, p }) => {
            let mut analyzer = analysis::Analyzer::new(db.display().to_string());
            analyzer.analyze_smallest_benches(p)?;
        }
        None => {}
    }

    Ok(())
}
