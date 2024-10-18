use crate::db::Db;
use crate::types::{Benchmark, Status as BenchStatus};

use glob::glob;
use log::error;
use serde_json::Value;
use std::os::unix::fs::symlink;
use std::process::Command;

pub(super) fn process(db: &mut Db, benchmark: &Benchmark) -> () {
    // let mut stmt = conn.prepare("INSERT INTO \"sources\" (path, prefix) VALUES (?1, ?2)")?;

    // let build_dir = ARGS.build_dir;
    // let build_dir = build_dir.canonicalize().unwrap().display().to_string();
    // let pattern = format!("{}/**/*.gcno", build_dir);

    // for entry in glob(&pattern).expect("Failed to read glob pattern") {
    //     if let Ok(file) = entry {
    //         // FIXME: This will be of the form src/CMakeFiles/cvc5-obj.dir/.../*.cpp
    //         // It would be best if I could also strip the CMakeFiles/cvc5-obj.dir
    //         // But first I will have to check it for consistency
    //         let file = file
    //             .strip_prefix(build_dir)
    //             .expect("Error while stripping common prefix from gcno file");
    //         let src_file = file.to_str().unwrap();
    //         let src_file = &src_file[..src_file.len() - 5];
    //         stmt.execute(params![src_file])?;
    //     }
    // }

    // FIXME: Symlink the gcno files here

    let prefix_dir = benchmark.prefix.display().to_string();
    let pattern = format!("{}/**/*.gcda", prefix_dir);

    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(gcda_file) = entry {
            let gcno_file = gcda_file.to_str().unwrap();
            let gcno_file = format!("{}.gcno", &gcno_file[..gcno_file.len() - 5]);
            let gcno_file_src = gcno_file.strip_prefix(&prefix_dir);
            symlink(&gcno_file, &gcno_file).expect("Error while trying to create symlink");

            let args = ["--json", "--stdout", gcda_file.to_str().unwrap()];
            let output = Command::new("gcov")
                .args(&args)
                .output()
                .expect("Could not capture output of gcov...");
            if !output.status.success() {
                error!(
                    "Gcov failed with error!\n GCDA File: {:?} \n ERROR: {:?}",
                    &gcda_file, &output.stderr
                );
                return;
            }

            // FIXME: Postprocessing
            let gcov_json: Value =
                serde_json::from_slice(&output.stdout).expect("Error parsing gcov json output");
        }
    }

    db.update_benchmark_status(benchmark.id, BenchStatus::Done)
        .unwrap();
}
