use crate::args::{ARGS, TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::types::{
    Benchmark, FilePosition, GcovBranchResult, GcovFuncResult, GcovLineResult, ResultT,
};

use glob::glob;
use log::error;
use serde::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::fs::remove_file;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::symlink;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// Maps from SrcFileName -> Line/Function/Branch Identifier -> Result
pub type GcovRes = HashMap<
    Arc<String>,
    (
        HashMap<Arc<String>, Arc<GcovFuncResult>>,
        HashMap<u32, Arc<GcovLineResult>>,
        HashMap<u32, Arc<GcovBranchResult>>,
    ),
>;

const CHUNK_SIZE: usize = 1;

pub(super) fn process(benchmark: &Benchmark) -> GcovRes {
    let prefix_dir = match benchmark.prefix.clone() {
        None => ARGS.build_dir.clone(),
        Some(p) => p,
    }
    .display()
    .to_string();
    let pattern = format!("{}/**/*.gcda", prefix_dir);

    let mut files = vec![];
    for f in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(gcda_file) = f {
            files.push(gcda_file)
        }
    }

    let mut ires = vec![];
    for gcda_chunk in files.chunks(CHUNK_SIZE) {
        let mut gcno_symlinks = vec![];
        for gcda_file in gcda_chunk {
            if ARGS.individual_prefixes {
                let gcno_file_dst = gcda_file.to_str().unwrap();
                let gcno_file_dst = format!("{}.gcno", &gcno_file_dst[..gcno_file_dst.len() - 5]);
                let gcno_file_src = gcno_file_dst.strip_prefix(&prefix_dir).unwrap();
                symlink(&gcno_file_src, &gcno_file_dst).unwrap_or(());
                gcno_symlinks.push(gcno_file_dst);
            }
        }

        let chunk_args: Vec<&str> = gcda_chunk.iter().map(|p| p.to_str().unwrap()).collect();
        let args = ["--json", "--stdout"]; // gcda_file.to_str().unwrap()];
        let output = Command::new("gcov")
            .args(&args)
            .args(&chunk_args)
            .output()
            .expect("Could not capture output of gcov...");
        let stderr = String::from_utf8(output.stderr).unwrap();
        if !output.status.success() {
            error!(
                "Gcov failed with error!\n GCDA Files: {:?} \n ERROR: {:?}",
                &gcda_chunk, stderr
            );
            continue;
        }

        if CHUNK_SIZE == 1 {
            let gcov_json: GcovJson = serde_json::from_slice(output.stdout.as_slice())
                .expect("Error parsing gcov json output");

            ires.push(interpret_gcov(&gcov_json).expect("Error while interpreting gcov output"));
        } else {
            // Read lines from the stdout
            let reader = BufReader::new(output.stdout.as_slice());
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let gcov_json: GcovJson =
                            serde_json::from_str(&line).expect("Error parsing gcov json output");

                        ires.push(
                            interpret_gcov(&gcov_json)
                                .expect("Error while interpreting gcov output"),
                        );
                    }
                    Err(_) => error!("An error occurred while reading the output lines of gcov"),
                }
            }
        }

        // Delete the gcda file gcno file if it was symlinked
        for gcda_file in gcda_chunk {
            remove_file(gcda_file).unwrap();
        }
        for gcno_symlink in gcno_symlinks {
            remove_file(gcno_symlink).unwrap();
        }
    }

    return merge_gcov(ires);

    // let result: GcovRes = ires
    //     .iter()
    //     .map(|(key, value)| {
    //         (
    //             Arc::clone(key),
    //             (
    //                 value.0.values().cloned().collect(),
    //                 value.1.values().cloned().collect(),
    //                 value.2.values().cloned().collect(),
    //             ),
    //         )
    //     })
    //     .collect();

    // return result;
}

pub fn merge_gcov(ires: Vec<GcovRes>) -> GcovRes {
    let mut res: GcovRes = HashMap::new();
    for iires in ires {
        for (key, value) in iires {
            res.entry(key)
                .and_modify(|pvalue| {
                    if TRACK_FUNCS.clone() {
                        for (k, v) in &value.0 {
                            pvalue
                                .0
                                .entry(Arc::clone(k))
                                .and_modify(|e| {
                                    let v_usage = v.usage.load(Ordering::SeqCst);
                                    e.usage.fetch_max(v_usage, Ordering::SeqCst);
                                })
                                .or_insert(Arc::clone(v));
                        }
                    }

                    if TRACK_LINES.clone() {
                        for (k, v) in &value.1 {
                            pvalue
                                .1
                                .entry(*k)
                                .and_modify(|e| {
                                    let v_usage = v.usage.load(Ordering::SeqCst);
                                    e.usage.fetch_max(v_usage, Ordering::SeqCst);
                                })
                                .or_insert(Arc::clone(v));
                        }
                    }

                    if TRACK_BRANCHES.clone() {
                        unreachable!();
                        // for (k, v) in value.2 {
                        //     pvalue
                        //         .2
                        //         .entry(k)
                        //         .and_modify(|_e| {
                        //             if ARGS.mode == CoverageMode::Full {
                        //             }
                        //         })
                        //         .or_insert(v);
                        // }
                    }
                })
                .or_insert(value);
        }
    }
    return res;
}

fn interpret_gcov(json: &GcovJson) -> ResultT<GcovRes> {
    let mut result: GcovRes = HashMap::new();

    for file in &json.files {
        // Filter out libraries unless specified otherwise
        if !ARGS.no_ignore_libs && file.file.starts_with("/usr/include") {
            continue;
        }
        let mut funcs: HashMap<Arc<String>, Arc<GcovFuncResult>> = HashMap::new();
        if TRACK_FUNCS.clone() {
            for function in &file.functions {
                let usage = (function.execution_count as u32 > 0) as u32;
                let name = function.demangled_name.clone();
                funcs.insert(
                    Arc::from(name.clone()),
                    Arc::from(GcovFuncResult {
                        name,
                        start: FilePosition {
                            line: function.start_line,
                            col: function.start_column,
                        },
                        end: FilePosition {
                            line: function.end_line,
                            col: function.end_column,
                        },
                        usage: AtomicU32::from(usage),
                    }),
                );
            }
        }

        let mut lines: HashMap<u32, Arc<GcovLineResult>> = HashMap::new();
        if TRACK_LINES.clone() {
            for line in &file.lines {
                let usage = (line.count as u32 > 0) as u32;
                lines.insert(
                    line.line_number,
                    Arc::from(GcovLineResult {
                        line_no: line.line_number,
                        usage: AtomicU32::from(usage),
                    }),
                );
            }
        }

        let branches: HashMap<u32, Arc<GcovBranchResult>> = HashMap::new();
        if TRACK_BRANCHES.clone() {
            // TODO: Add support for branch tracking
            unimplemented!("Branch tracking not yet supported")
        }

        result.insert(Arc::from(file.file.clone()), (funcs, lines, branches));
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
    execution_count: f64,
    // name: String,
    start_column: u32,
    start_line: u32,
}

#[derive(Debug, Deserialize)]
struct LineElement {
    line_number: u32,
    count: f64,
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
