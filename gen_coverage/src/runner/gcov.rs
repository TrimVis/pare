use crate::args::{ARGS, TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::types::{
    Benchmark, FilePosition, GcovBranchResult, GcovFuncResult, GcovLineResult, ResultT,
};

use bitvec::prelude::*;
use glob::glob;
use log::{error, info};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json;

use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs::remove_file;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::symlink;
use std::process::{exit, Command};

// Maps from SrcFileName -> Line/Function/Branch Identifier -> Result
pub type GcovRes = HashMap<
    Box<String>,
    (
        HashMap<(u32, u32), RefCell<GcovFuncResult>>,
        HashMap<u32, RefCell<GcovLineResult>>,
        HashMap<u32, RefCell<GcovBranchResult>>,
    ),
>;

pub type GcovBitvec = HashMap<
    Box<String>,
    (
        HashMap<(u32, u32), BitVec<u8, Msb0>>,
        HashMap<u32, BitVec<u8, Msb0>>,
        HashMap<u32, BitVec<u8, Msb0>>,
    ),
>;

pub(super) fn res_to_bitvec(
    gcov_bitvec: &mut GcovBitvec,
    no_benchmarks: usize,
    benchmark_id: usize,
    result: &GcovRes,
) {
    for (key, value) in result {
        gcov_bitvec
            .borrow_mut()
            .entry(key.clone())
            .and_modify(|pvalue| {
                for (k, v) in &value.0 {
                    if v.borrow().usage > 0 {
                        pvalue
                            .0
                            .entry(*k)
                            .and_modify(|e| {
                                e.set(benchmark_id - 1, true);
                            })
                            .or_insert(bitvec![u8, Msb0; 0; no_benchmarks]);
                    }
                }

                for (k, v) in &value.1 {
                    if v.borrow().usage > 0 {
                        pvalue
                            .1
                            .entry(*k)
                            .and_modify(|e| {
                                e.set(benchmark_id - 1, true);
                            })
                            .or_insert(bitvec![u8, Msb0; 0; no_benchmarks]);
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
            .or_insert((HashMap::new(), HashMap::new(), HashMap::new()));
    }
}

const CHUNK_SIZE: usize = 20;

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

    let mut ires: Option<GcovRes> = None;
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
        let args = ["--json-format", "--stdout"]; // gcda_file.to_str().unwrap()];
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

        // let mut deserializer = JsonDeserializer::from_slice(output.stdout.as_slice());
        let reader = BufReader::new(output.stdout.as_slice());

        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    match serde_json::from_str(&line) {
                        Ok(gcov_json) => {
                            let new_res = interpret_gcov(&gcov_json)
                                .expect("Error while interpreting gcov output");
                            match ires.borrow_mut() {
                                Some(r) => {
                                    merge_gcov(r, new_res, MergeKind::MAX);
                                }
                                None => {
                                    ires = Some(new_res);
                                }
                            };
                        }
                        Err(e) => {
                            error!("{}\n\nCould not parse GCOV json:\n{}", line, e);
                            exit(1);
                        }
                    }
                }
                Err(_) => {
                    error!("Could not parse GCOV output line")
                }
            }
        }

        // Delete the gcda file gcno file if it was symlinked
        for gcda_file in gcda_chunk {
            remove_file(gcda_file)
                .unwrap_or_else(|e| error!("Could not remove gcda file: {:?}", e));
        }
        for gcno_symlink in gcno_symlinks {
            remove_file(gcno_symlink)
                .unwrap_or_else(|e| error!("Could not remove symlink: {:?}", e));
        }
    }

    if ires.is_some() {
        ires.unwrap()
    } else {
        error!("No result created...");
        HashMap::new()
    }
}

#[derive(PartialEq)]
pub enum MergeKind {
    SUM,
    MAX,
}

pub fn merge_gcov(res0: &mut GcovRes, res1: GcovRes, kind: MergeKind) {
    for (key, value) in res1 {
        res0.borrow_mut()
            .entry(key)
            .and_modify(|pvalue| {
                for (k, v) in &value.0 {
                    pvalue
                        .0
                        .entry(*k)
                        .and_modify(|e| {
                            let v_usage = v.borrow().usage;
                            let e = e.get_mut();
                            e.usage = if kind == MergeKind::MAX {
                                e.usage.max(v_usage)
                            } else {
                                e.usage + v_usage
                            };
                        })
                        .or_insert(RefCell::clone(v));
                }

                for (k, v) in &value.1 {
                    pvalue
                        .1
                        .entry(*k)
                        .and_modify(|e| {
                            let v_usage = v.borrow().usage;
                            let e = e.get_mut();
                            let new_val = if kind == MergeKind::MAX {
                                e.usage.max(v_usage)
                            } else {
                                e.usage + v_usage
                            };
                            e.usage = new_val;
                        })
                        .or_insert(RefCell::clone(v));
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

fn interpret_gcov(json: &GcovJson) -> ResultT<GcovRes> {
    let mut result: GcovRes = HashMap::new();

    for file in &json.files {
        // Ignore include files and build dir files, as we can not optimize over them anyways
        if !ARGS.no_ignore_libs
            && (file.file.starts_with("/usr/include")
                || file.file.starts_with(&ARGS.build_dir.display().to_string()))
        {
            continue;
        }

        // Filter out libraries unless specified otherwise
        let mut funcs: HashMap<(u32, u32), RefCell<GcovFuncResult>> = HashMap::new();
        if let Some(fs) = &file.functions {
            for function in fs {
                let usage = (function.execution_count as u32 > 0) as u32;
                let name = function.demangled_name.clone();
                funcs.insert(
                    (function.start_line, function.start_column),
                    RefCell::from(GcovFuncResult {
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
                    }),
                );
            }
        }

        let mut lines: HashMap<u32, RefCell<GcovLineResult>> = HashMap::new();
        if let Some(ls) = &file.lines {
            for line in ls {
                let usage = (line.count as u32 > 0) as u32;
                lines.insert(
                    line.line_number,
                    RefCell::from(GcovLineResult {
                        line_no: line.line_number,
                        usage,
                    }),
                );
            }
        }

        let branches: HashMap<u32, RefCell<GcovBranchResult>> = HashMap::new();
        if TRACK_BRANCHES.clone() {
            // TODO: Add support for branch tracking
            unimplemented!("Branch tracking not yet supported")
        }

        result.insert(Box::from(file.file.clone()), (funcs, lines, branches));
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

#[derive(Debug)]
struct FileElement {
    file: String,
    functions: Option<Vec<FunctionElement>>,
    lines: Option<Vec<LineElement>>,
}
impl<'de> Deserialize<'de> for FileElement {
    fn deserialize<D>(deserializer: D) -> Result<FileElement, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(FileElementVisitor)
    }
}

struct FileElementVisitor;

impl<'de> Visitor<'de> for FileElementVisitor {
    type Value = FileElement;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a map representing FileElement")
    }

    fn visit_map<V>(self, mut map: V) -> Result<FileElement, V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut functions = None;
        let mut lines = None;
        let mut file = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "file" => {
                    if file.is_some() {
                        return Err(de::Error::duplicate_field("file"));
                    }
                    file = Some(map.next_value()?);
                }
                "functions" => {
                    if functions.is_some() {
                        return Err(de::Error::duplicate_field("functions"));
                    }
                    if TRACK_FUNCS.clone() {
                        functions = Some(map.next_value()?);
                    } else {
                        let _ = map.next_value::<de::IgnoredAny>()?;
                    }
                }
                "lines" => {
                    if lines.is_some() {
                        return Err(de::Error::duplicate_field("lines"));
                    }
                    if TRACK_LINES.clone() {
                        lines = Some(map.next_value()?);
                    } else {
                        let _ = map.next_value::<de::IgnoredAny>()?;
                    }
                }
                _ => {
                    // Skip unknown fields
                    let _ = map.next_value::<de::IgnoredAny>()?;
                }
            }
        }

        if file.is_none() {
            return Err(de::Error::missing_field("file"));
        }

        Ok(FileElement {
            file: file.unwrap(),
            functions,
            lines,
        })
    }
}
