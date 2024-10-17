use glob::glob;
use rand::{seq::SliceRandom, thread_rng};
use serde_json::Value;
use std::collections::HashMap;
use std::process::exit;

use crate::args::ARGS;

/// Sample files from the benchmark directory.
pub fn sample_files(sample_size: &str) -> Vec<String> {
    // Verify if the benchmark directory exists
    if !ARGS.benchmark_dir.is_dir() {
        eprintln!("Error: Directory {} does not exist.", ARGS.benchmark_dir.display().to_string());
        exit(1);
    }

    // Collect all files with .smt2 extension
    let pattern = format!("{}/**/*.smt2", ARGS.benchmark_dir.display().to_string());
    let all_files: Vec<String> = glob(&pattern)
        .expect("Failed to read glob pattern")
        .filter_map(Result::ok)
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    let total_files = all_files.len();

    if sample_size == "all" {
        let mut rng = thread_rng();
        let mut shuffled_files = all_files.clone();
        shuffled_files.shuffle(&mut rng);
        shuffled_files
    } else {
        let sample_size_int: usize = sample_size.parse().expect("Invalid sample size");
        if sample_size_int > total_files {
            eprintln!(
                "Error: Requested sample size ({}) is greater than the total number of files ({}) in the directory.",
                sample_size_int, total_files
            );
            exit(1);
        }
        let mut rng = thread_rng();
        all_files
            .choose_multiple(&mut rng, sample_size_int)
            .cloned()
            .collect()
    }
}

/// Combine two dictionaries by adding their values. Example: combine_dicts({"a":1,"b":0}, {"a":2}) == {"a":3,"b":0}.
pub fn combine_dicts(
    dict1: &HashMap<String, i64>,
    dict2: &HashMap<String, i64>,
) -> HashMap<String, i64> {
    let mut result = dict1.clone();
    for (k, v) in dict2 {
        *result.entry(k.clone()).or_insert(0) += v;
    }
    result
}

/// Combine two lists by adding their elements, ignoring value. Example: combine_lists([4,1], [2,2,0]) == [2,2].
pub fn combine_lists(list1: &[i64], list2: &[i64]) -> Vec<i64> {
    let (blist, slist) = if list1.len() > list2.len() {
        (list1.to_vec(), list2)
    } else {
        (list2.to_vec(), list1)
    };

    let mut result = blist.clone();
    for (i, &s) in slist.iter().enumerate() {
        result[i] += s;
    }
    result
}

/// Combine two reports. If `exec_one` is true, special handling for execution coverage is applied.
pub fn combine_reports(base: &mut Value, overlay: &Value, exec_one: bool) {
    let base_sources = base["sources"].as_object_mut().unwrap();
    let overlay_sources = overlay["sources"].as_object().unwrap();

    for (source, scov) in overlay_sources {
        if !base_sources.contains_key(source) {
            if exec_one {
                base_sources.insert(source.clone(), serde_json::json!({}));
            } else {
                base_sources.insert(source.clone(), scov.clone());
                continue;
            }
        }

        let base_source = base_sources.get_mut(source).unwrap();
        let scov_obj = scov.as_object().unwrap();

        for (test_name, tcov) in scov_obj {
            if !base_source.get(test_name).is_some() {
                base_source[test_name] = serde_json::json!({
                    "lines": {},
                    "branches": {},
                    "functions": {}
                });
            }

            let base_test = base_source
                .get_mut(test_name)
                .unwrap()
                .as_object_mut()
                .unwrap();
            let tcov_obj = tcov.as_object().unwrap();

            // Handle lines
            let tcov_lines = tcov_obj["lines"].as_object().unwrap();
            let base_lines = base_test["lines"].as_object_mut().unwrap();
            if exec_one {
                for (line, value) in tcov_lines {
                    base_lines.insert(
                        line.clone(),
                        serde_json::json!(if value.as_i64().unwrap() > 0 { 1 } else { 0 }),
                    );
                }
            } else {
                for (line, value) in tcov_lines {
                    let line_entry = base_lines
                        .entry(line.clone())
                        .or_insert(serde_json::json!(0));
                    *line_entry =
                        serde_json::json!(line_entry.as_i64().unwrap() + value.as_i64().unwrap());
                }
            }

            // Handle branches
            let tcov_branches = tcov_obj["branches"].as_object().unwrap();
            let base_branches = base_test["branches"].as_object_mut().unwrap();
            for (branch, cov) in tcov_branches {
                let cov_list = cov.as_array().unwrap();
                if exec_one {
                    let cov_bool = cov_list
                        .iter()
                        .map(|v| if v.as_i64().unwrap() > 0 { 1 } else { 0 })
                        .collect::<Vec<_>>();
                    base_branches.insert(branch.clone(), serde_json::json!(cov_bool));
                } else {
                    if base_branches.contains_key(branch) {
                        let base_branch = base_branches
                            .get_mut(branch)
                            .unwrap()
                            .as_array_mut()
                            .unwrap();
                        let new_cov = combine_lists(
                            &base_branch
                                .iter()
                                .map(|v| v.as_i64().unwrap())
                                .collect::<Vec<_>>(),
                            &cov_list
                                .iter()
                                .map(|v| v.as_i64().unwrap())
                                .collect::<Vec<_>>(),
                        );
                        *base_branch = new_cov.into_iter().map(|v| serde_json::json!(v)).collect();
                    } else {
                        base_branches.insert(branch.clone(), serde_json::json!(cov_list));
                    }
                }
            }

            // Handle functions
            let tcov_functions = tcov_obj["functions"].as_object().unwrap();
            let base_functions = base_test["functions"].as_object_mut().unwrap();
            for (function, cov) in tcov_functions {
                let cov_obj = cov.as_object().unwrap();
                if exec_one {
                    base_functions.insert(function.clone(), serde_json::json!({
                        "run_count": if cov_obj["run_count"].as_i64().unwrap() > 0 { 1 } else { 0 }
                    }));
                } else {
                    if base_functions.contains_key(function) {
                        base_functions.get_mut(function).unwrap()["run_count"] = serde_json::json!(
                            base_functions[function]["run_count"].as_i64().unwrap()
                                + cov_obj["run_count"].as_i64().unwrap()
                        );
                    } else {
                        base_functions.insert(function.clone(), cov.clone());
                    }
                }
            }
        }
    }
}
