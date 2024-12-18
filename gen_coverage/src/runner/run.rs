use crate::args::EXEC_PLACEHOLDER;
use crate::types::{Benchmark, BenchmarkRun};

use log::{error, info};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

pub(super) fn process(benchmark: &Benchmark) -> Option<BenchmarkRun> {
    // Assumes that the full path is always passed
    let exec = PathBuf::from(&EXEC_PLACEHOLDER[0]);
    let cmd = &mut Command::new(&exec);
    if let Some(prefix) = &benchmark.prefix {
        cmd.env("GCOV_PREFIX", prefix.display().to_string());
    }
    // Replace {} in our template args with the file
    let args = EXEC_PLACEHOLDER[1..].iter().map(|c| {
        if c == "{}" {
            benchmark.path.display().to_string()
        } else {
            c.clone()
        }
    });
    cmd.args(args.clone());

    let start = Instant::now();
    let output = cmd
        .output()
        .expect("Could not capture output of execution...");
    let duration = start.elapsed();

    let stderr = String::from_utf8(output.stderr).unwrap();
    let exit_code = output
        .status
        .code()
        .unwrap_or(output.status.signal().unwrap_or(100000));
    if !output.status.success() {
        error!(
            "Execution failed with error ({:?})!\n Benchmark File: {:?} \n ERROR: {:?}",
            output.status, &benchmark.path, &stderr
        );
        error!("Args: {:?}", args);
    } else {
        info!(
            "Benchmark run succeded! [{}] (File: {:?})",
            output.status, &benchmark.path
        );
    }

    return Some(BenchmarkRun {
        bench_id: benchmark.id,
        exit_code,
        time_ms: duration
            .as_millis()
            .try_into()
            .expect("Duration too long for 64 bits"),
        stdout: Some(String::from_utf8(output.stdout).expect("Error decoding execution stdout")),
        stderr: Some(stderr),
    });
}
