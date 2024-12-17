mod analysis;
mod remover;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = "../report.sqlite";

    let mut analyzer = analysis::Analyzer::new(db.to_string());
    analyzer.analyze()?;
    analyzer.visualize("./function_line_deviation.png")?;

    return Ok(());

    let remover = remover::Remover::new(db.to_string());
    remover.remove("optimization_result_p0_9900", true)?;

    Ok(())
}
