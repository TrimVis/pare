use crate::db::Db;
use crate::types::{Benchmark, BenchmarkRun, Status as BenchStatus};
use crate::ARGS;

use log::error;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub(super) fn process(db: &mut Db, cvc5cmd: &Path, benchmark: &Benchmark) -> () {
    let cmd = &mut Command::new(cvc5cmd);
    let cmd = cmd
        .env("GCOV_PREFIX", benchmark.prefix.display().to_string())
        .args(&[&ARGS.cvc5_args, &benchmark.path.display().to_string()]);

    let start = Instant::now();
    let output = cmd.output().expect("Could not capture output of cvc5...");
    let duration = start.elapsed();

    if !output.status.success() {
        error!(
            "CVC5 failed with error!\n Benchmark File: {:?} \n ERROR: {:?}",
            &benchmark.path, &output.stderr
        );
    }

    db.add_cvc5_run_result(BenchmarkRun {
        bench_id: benchmark.id,
        exit_code: output.status.code().unwrap(),
        time_ms: duration
            .as_millis()
            .try_into()
            .expect("Duration too long for 64 bits"),
        stdout: Some(String::from_utf8(output.stdout).expect("Error decoding cvc5 stdout")),
        stderr: Some(String::from_utf8(output.stderr).expect("Error decoding cvc5 stderr")),
    })
    .unwrap();
    db.update_benchmark_status(benchmark.id, BenchStatus::WaitingProcessing)
        .unwrap();
}
