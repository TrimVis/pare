use crate::args::{ARGS, TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::types::{
    Benchmark, FilePosition, GcovBranchResult, GcovFuncResult, GcovLineResult, ResultT,
};

use glob::glob;
use log::error;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json;
use serde_json::de::Deserializer as JsonDeserializer;
use std::collections::HashMap;
use std::fmt;
use std::fs::remove_file;
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

const CHUNK_SIZE: usize = 10;

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

        let mut deserializer = JsonDeserializer::from_slice(output.stdout.as_slice());

        while let Ok(gcov_json) = GcovJson::deserialize(&mut deserializer) {
            ires.push(interpret_gcov(&gcov_json).expect("Error while interpreting gcov output"));
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
}

pub fn merge_gcov(ires: Vec<GcovRes>) -> GcovRes {
    let mut res: GcovRes = HashMap::new();
    for iires in ires {
        for (key, value) in iires {
            res.entry(key)
                .and_modify(|pvalue| {
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

// fn interpret_gcov(json_data: &[u8]) -> ResultT<GcovRes> {
//     let gcov_json: GcovJson = serde_json::from_slice(json_data)?;
//
//     let mut result: GcovRes = HashMap::new();
//     for file in &gcov_json.files {
fn interpret_gcov(json: &GcovJson) -> ResultT<GcovRes> {
    let mut result: GcovRes = HashMap::new();

    for file in &json.files {
        // Filter out libraries unless specified otherwise
        let mut funcs: HashMap<Arc<String>, Arc<GcovFuncResult>> = HashMap::new();
        if let Some(fs) = &file.functions {
            for function in fs {
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
        if let Some(ls) = &file.lines {
            for line in ls {
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

        // A little hacky but we will enfore file to be parsed first, so we can skip parsing for
        // system libraries which is enabled by default
        let file = match map.next_key::<String>()?.unwrap().as_str() {
            "file" => {
                let next_value: String = map.next_value()?;
                if !ARGS.no_ignore_libs && next_value.starts_with("/usr/include") {
                    return Ok(FileElement {
                        file: next_value,
                        functions: None,
                        lines: None,
                    });
                }
                next_value
            }
            _ => return Err(de::Error::custom("file did not appear as first field!")),
        };

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "functions" => {
                    if functions.is_some() {
                        return Err(de::Error::duplicate_field("functions"));
                    }
                    if TRACK_FUNCS.clone() {
                        functions = Some(map.next_value()?);
                    } else {
                        // Skip the value
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
                        // Skip the value
                        let _ = map.next_value::<de::IgnoredAny>()?;
                    }
                }
                _ => {
                    // Skip unknown fields
                    let _ = map.next_value::<de::IgnoredAny>()?;
                }
            }
        }

        Ok(FileElement {
            file,
            functions,
            lines,
        })
    }
}
