use crate::args::{CoverageMode, ARGS, TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::types::{
    Benchmark, FilePosition, GcovBranchResult, GcovFuncResult, GcovLineResult, ResultT,
};

use glob::glob;
use log::error;
use serde::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::os::unix::fs::symlink;
use std::process::Command;

pub type GcovRes = HashMap<
    String,
    (
        Vec<GcovFuncResult>,
        Vec<GcovLineResult>,
        Vec<GcovBranchResult>,
    ),
>;

pub(super) fn process(benchmark: &Benchmark) -> GcovRes {
    let prefix_dir = match benchmark.prefix.clone() {
        None => ARGS.benchmark_dir.clone(),
        Some(p) => p,
    }
    .display()
    .to_string();
    let pattern = format!("{}/**/*.gcda", prefix_dir);

    let mut result: GcovRes = HashMap::new();
    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(gcda_file) = entry {
            if ARGS.individual_prefixes {
                let gcno_file_dst = gcda_file.to_str().unwrap();
                let gcno_file_dst = format!("{}.gcno", &gcno_file_dst[..gcno_file_dst.len() - 5]);
                let gcno_file_src = gcno_file_dst.strip_prefix(&prefix_dir).unwrap();
                symlink(&gcno_file_src, &gcno_file_dst).unwrap_or(());
            }

            let args = ["--json", "--stdout", gcda_file.to_str().unwrap()];
            let output = Command::new("gcov")
                .args(&args)
                .output()
                .expect("Could not capture output of gcov...");
            let stderr = String::from_utf8(output.stderr).unwrap();
            if !output.status.success() {
                error!(
                    "Gcov failed with error!\n GCDA File: {:?} \n ERROR: {:?}",
                    &gcda_file, stderr
                );
                continue;
            }
            let gcov_json: GcovJson =
                serde_json::from_slice(&output.stdout).expect("Error parsing gcov json output");

            for (key, value) in interpret_gcov(&gcov_json)
                .expect("Could not interpret gcov output properly")
                .iter_mut()
            {
                if !result.contains_key(key) {
                    result.insert(key.clone(), value.clone());
                } else {
                    let (funcs, lines, branches) = result.get_mut(key).unwrap();
                    if TRACK_FUNCS.clone() {
                        funcs.append(&mut value.0);
                    }
                    if TRACK_LINES.clone() {
                        lines.append(&mut value.1);
                    }
                    if TRACK_BRANCHES.clone() {
                        branches.append(&mut value.2)
                    }
                }
            }
        }
    }

    return result;
}

fn interpret_gcov(json: &GcovJson) -> ResultT<GcovRes> {
    let mut result: GcovRes = HashMap::new();

    for file in &json.files {
        // Filter out libraries unless specified otherwise
        if !ARGS.no_ignore_libs && file.file.starts_with("/usr/include") {
            continue;
        }
        let mut funcs: Vec<GcovFuncResult> = vec![];
        if TRACK_FUNCS.clone() {
            for function in &file.functions {
                let usage = if ARGS.mode == CoverageMode::Full {
                    function.execution_count
                } else {
                    (function.execution_count > 0) as u32
                };
                funcs.push(GcovFuncResult {
                    name: function.demangled_name.clone(),
                    start: FilePosition {
                        line: function.start_line,
                        col: function.start_column,
                    },
                    end: FilePosition {
                        line: function.end_line,
                        col: function.end_column,
                    },
                    usage,
                });
            }
        }

        let mut lines: Vec<GcovLineResult> = vec![];
        if TRACK_LINES.clone() {
            for line in &file.lines {
                let usage = if ARGS.mode == CoverageMode::Full {
                    line.count
                } else {
                    (line.count > 0) as u32
                };
                lines.push(GcovLineResult {
                    line_no: line.line_number,
                    usage,
                });
            }
        }

        let branches: Vec<GcovBranchResult> = vec![];
        if TRACK_BRANCHES.clone() {
            // TODO: Add support for branch tracking
            unimplemented!("Branch tracking not yet supported")
        }

        result.insert(file.file.clone(), (funcs, lines, branches));
    }

    Ok(result)
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GcovJson {
    current_working_directory: String,
    data_file: String,
    format_version: String,
    gcc_version: String,
    files: Vec<FileElement>,
}

#[derive(Debug, Deserialize)]
struct FileElement {
    file: String,
    functions: Vec<FunctionElement>,
    lines: Vec<LineElement>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FunctionElement {
    blocks: u32,
    blocks_executed: u32,
    demangled_name: String,
    end_column: u32,
    end_line: u32,
    execution_count: u32,
    name: String,
    start_column: u32,
    start_line: u32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct LineElement {
    line_number: u32,
    count: u32,
    function_name: Option<String>, //TODO: Also incorporate this information into the DB
    unexecuted_block: bool,
    branches: Option<Vec<BranchElement>>,
    calls: Option<Vec<CallElement>>,
    conditions: Option<Vec<ConditionElement>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BranchElement {
    count: u32,
    destination_block_id: u32,
    fallthrough: bool,
    source_block_id: u32,
    r#throw: bool,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CallElement {
    destination_block_id: u32,
    returned: u32,
    source_block_id: u32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ConditionElement {
    count: u32,
    covered: u32,
    not_covered_true: Vec<u32>,
    not_covered_false: Vec<u32>,
}
