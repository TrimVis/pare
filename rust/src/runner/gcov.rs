use crate::args::{CoverageMode, ARGS, TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::types::{
    Benchmark, FilePosition, GcovBranchResult, GcovFuncResult, GcovLineResult, ResultT,
};

use glob::glob;
use log::error;
use serde::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::fs::remove_file;
use std::os::unix::fs::symlink;
use std::process::Command;

pub type GcovIRes = HashMap<
    String,
    (
        HashMap<String, GcovFuncResult>,
        HashMap<u32, GcovLineResult>,
        HashMap<u32, GcovBranchResult>,
    ),
>;

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
        None => ARGS.build_dir.clone(),
        Some(p) => p,
    }
    .display()
    .to_string();
    let pattern = format!("{}/**/*.gcda", prefix_dir);

    let mut ires: GcovIRes = HashMap::new();
    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(gcda_file) = entry {
            let gcno_symlink = if ARGS.individual_prefixes {
                let gcno_file_dst = gcda_file.to_str().unwrap();
                let gcno_file_dst = format!("{}.gcno", &gcno_file_dst[..gcno_file_dst.len() - 5]);
                let gcno_file_src = gcno_file_dst.strip_prefix(&prefix_dir).unwrap();
                symlink(&gcno_file_src, &gcno_file_dst).unwrap_or(());
                Some(gcno_file_dst)
            } else {
                None
            };

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
                if let Some((funcs, lines, branches)) = ires.get_mut(key) {
                    // let (funcs, lines, branches) = ires.get_mut(key).unwrap();
                    let (nfuncs, nlines, nbranches) = value;
                    if TRACK_FUNCS.clone() {
                        for (k, v) in nfuncs {
                            if let Some(fv) = funcs.get_mut(k) {
                                if ARGS.mode == CoverageMode::Full {
                                    fv.usage += v.usage;
                                }
                            } else {
                                funcs.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    if TRACK_LINES.clone() {
                        for (k, v) in nlines {
                            if let Some(lv) = lines.get_mut(k) {
                                if ARGS.mode == CoverageMode::Full {
                                    lv.usage += v.usage;
                                }
                            } else {
                                lines.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    if TRACK_BRANCHES.clone() {
                        for (k, v) in nbranches {
                            if let Some(_) = branches.get_mut(k) {
                                if ARGS.mode == CoverageMode::Full {
                                    // FIXME: Uncomment this line as soon as branch support is a thing
                                    // bv.usage += v.usage;
                                    unreachable!();
                                }
                            } else {
                                branches.insert(k.clone(), v.clone());
                            }
                        }
                    }
                } else {
                    ires.insert(key.clone(), value.clone());
                }
            }

            // Delete the gcda file gcno file if it was symlinked
            remove_file(gcda_file);
            if let Some(file) = gcno_symlink {
                remove_file(file);
            }
        }
    }

    let result: GcovRes = ires
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                (
                    value.0.values().cloned().collect(),
                    value.1.values().cloned().collect(),
                    value.2.values().cloned().collect(),
                ),
            )
        })
        .collect();

    return result;
}

fn interpret_gcov(json: &GcovJson) -> ResultT<GcovIRes> {
    let mut result: GcovIRes = HashMap::new();

    for file in &json.files {
        // Filter out libraries unless specified otherwise
        if !ARGS.no_ignore_libs && file.file.starts_with("/usr/include") {
            continue;
        }
        let mut funcs: HashMap<String, GcovFuncResult> = HashMap::new();
        if TRACK_FUNCS.clone() {
            for function in &file.functions {
                let usage = if ARGS.mode == CoverageMode::Full {
                    function.execution_count
                } else {
                    (function.execution_count > 0) as u32
                };
                let name = function.demangled_name.clone();
                funcs.insert(
                    name.clone(),
                    GcovFuncResult {
                        name,
                        start: FilePosition {
                            line: function.start_line,
                            col: function.start_column,
                        },
                        end: FilePosition {
                            line: function.end_line,
                            col: function.end_column,
                        },
                        usage,
                    },
                );
            }
        }

        let mut lines: HashMap<u32, GcovLineResult> = HashMap::new();
        if TRACK_LINES.clone() {
            for line in &file.lines {
                let usage = if ARGS.mode == CoverageMode::Full {
                    line.count
                } else {
                    (line.count > 0) as u32
                };
                lines.insert(
                    line.line_number,
                    GcovLineResult {
                        line_no: line.line_number,
                        usage,
                    },
                );
            }
        }

        let branches: HashMap<u32, GcovBranchResult> = HashMap::new();
        if TRACK_BRANCHES.clone() {
            // TODO: Add support for branch tracking
            unimplemented!("Branch tracking not yet supported")
        }

        result.insert(file.file.clone(), (funcs, lines, branches));
    }

    Ok(result)
}

#[derive(Debug, Deserialize)]
struct GcovJson {
    // current_working_directory: String,
    // data_file: String,
    // format_version: String,
    // gcc_version: String,
    files: Vec<FileElement>,
}

#[derive(Debug, Deserialize)]
struct FileElement {
    file: String,
    functions: Vec<FunctionElement>,
    lines: Vec<LineElement>,
}

#[derive(Debug, Deserialize)]
struct FunctionElement {
    // blocks: u32,
    // blocks_executed: u32,
    demangled_name: String,
    end_column: u32,
    end_line: u32,
    execution_count: u32,
    // name: String,
    start_column: u32,
    start_line: u32,
}

#[derive(Debug, Deserialize)]
struct LineElement {
    line_number: u32,
    count: u32,
    // function_name: Option<String>, //TODO: Also incorporate this information into the DB
    // unexecuted_block: bool,
    // branches: Option<Vec<BranchElement>>,
    // calls: Option<Vec<CallElement>>,
    // conditions: Option<Vec<ConditionElement>>,
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
