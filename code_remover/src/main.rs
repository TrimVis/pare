mod analysis;
mod remover;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use clap_num::number_range;
use ordered_float::OrderedFloat;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long)]
    db: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

fn valid_p_value(s: &str) -> Result<OrderedFloat<f32>, String> {
    let min = OrderedFloat(0.0);
    let max = OrderedFloat(0.99);
    number_range(s, min, max)
}

#[derive(Subcommand)]
enum Commands {
    /// Visualizes the distribution of function start and end lines, versus what is actually found,
    /// with the current detection method
    Visualize {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Remove the functions that have been determined as unneccessary by our optimization step
    Remove {
        #[arg(short, long, value_parser=valid_p_value)]
        p_value: OrderedFloat<f32>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let db = cli.db.to_str().unwrap();

    match cli.command {
        Some(Commands::Visualize { output }) => {
            let mut analyzer = analysis::Analyzer::new(db.to_string());
            analyzer.analyze()?;
            analyzer.visualize(
                output
                    .unwrap_or(PathBuf::from("./function_line_deviation.png"))
                    .to_str()
                    .unwrap(),
            )?;
        }
        Some(Commands::Remove { p_value }) => {
            let remover = remover::Remover::new(db.to_string());
            let table_name = format!(
                "optimization_result_p0_{}",
                (p_value * OrderedFloat(1000.0)).round() as u32
            );
            remover.remove(&table_name, true)?;
        }
        None => {}
    }

    Ok(())
}
