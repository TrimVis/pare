use crate::types::{Benchmark, BenchmarkRun};
use crate::ARGS;

use log::error;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub(super) fn process(cvc5cmd: &Path, benchmark: &Benchmark) -> Option<BenchmarkRun> {
    let mut cmd = &mut Command::new(cvc5cmd);
    if let Some(prefix) = &benchmark.prefix {
        cmd = cmd.env("GCOV_PREFIX", prefix.display().to_string());
    }
    // TODO: Use the argument and don't harcode this
    cmd = cmd.args(&["--tlimit", "5000", &benchmark.path.display().to_string()]);

    let start = Instant::now();
    let output = cmd.output().expect("Could not capture output of cvc5...");
    let duration = start.elapsed();

    let stderr = String::from_utf8(output.stderr).unwrap();
    if !output.status.success() {
        error!(
            "CVC5 failed with error!\n Benchmark File: {:?} \n ERROR: {:?}",
            &benchmark.path, &stderr
        );
        error!(
            "Args: {:?}",
            [&ARGS.cvc5_args, &benchmark.path.display().to_string()]
        );
    }

    return Some(BenchmarkRun {
        bench_id: benchmark.id,
        exit_code: output.status.code().unwrap_or(5),
        time_ms: duration
            .as_millis()
            .try_into()
            .expect("Duration too long for 64 bits"),
        stdout: Some(String::from_utf8(output.stdout).expect("Error decoding cvc5 stdout")),
        stderr: Some(stderr),
    });
}
