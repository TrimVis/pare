use serde_json::Value;
use std::collections::HashMap;

pub fn distill_function(function_raw: &Value, functions: &mut HashMap<String, HashMap<String, i64>>) {
    let function_name = function_raw["name"].as_str().unwrap();
    let start_line = function_raw["start_line"].as_i64().unwrap();
    let execution_count = function_raw["execution_count"].as_i64().unwrap();

    if !functions.contains_key(function_name) {
        let mut func_info = HashMap::new();
        func_info.insert("start_line".to_string(), start_line);
        func_info.insert("run_count".to_string(), execution_count);
        functions.insert(function_name.to_string(), func_info);
    } else {
        let func_info = functions.get_mut(function_name).unwrap();
        *func_info.get_mut("run_count").unwrap() += execution_count;
    }
}

pub fn empty_branch_set(branch1: &Value, branch2: &Value) -> bool {
    branch1["count"].as_i64().unwrap() == 0 && branch2["count"].as_i64().unwrap() == 0
}

pub fn matching_branch_set(branch1: &Value, branch2: &Value) -> bool {
    branch1["count"].as_i64().unwrap() == branch2["count"].as_i64().unwrap()
}

pub fn filter_exceptional_branches(branches: &Vec<Value>) -> Vec<Value> {
    let mut filtered_branches = Vec::new();
    let mut exception_branch = false;

    for i in (0..branches.len()).step_by(2) {
        if i + 1 >= branches.len() {
            filtered_branches.push(branches[i].clone());
            break;
        }

        if branches[i + 1]["throw"].as_bool().unwrap_or(false) {
            exception_branch = true;
            continue;
        }

        if exception_branch && empty_branch_set(&branches[i], &branches[i + 1])
            && filtered_branches.len() >= 2
            && matching_branch_set(&filtered_branches[filtered_branches.len() - 1], &filtered_branches[filtered_branches.len() - 2])
        {
            return Vec::new();
        }

        filtered_branches.push(branches[i].clone());
        filtered_branches.push(branches[i + 1].clone());
    }

    filtered_branches
}

pub fn distill_line(
    line_raw: &Value,
    lines: &mut HashMap<i64, i64>,
    branches: &mut HashMap<i64, Vec<i64>>,
) {
    let line_number = line_raw["line_number"].as_i64().unwrap();
    let mut count = line_raw["count"].as_i64().unwrap();

    if count < 0 {
        if let Some(function_name) = line_raw.get("function_name") {
            println!(
                "WARN: Ignoring negative count found in '{}'.",
                function_name.as_str().unwrap()
            );
        } else {
            println!("WARN: Ignoring negative count.");
        }
        count = 0;
    }

    *lines.entry(line_number).or_insert(0) += count;

    let line_branches = line_raw["branches"].as_array().unwrap();
    for (i, branch) in line_branches.iter().enumerate() {
        let branch_count = branch["count"].as_i64().unwrap();
        let branch_entry = branches.entry(line_number).or_insert_with(|| vec![0; line_branches.len()]);
        if branch_entry.len() < line_branches.len() {
            branch_entry.resize(line_branches.len(), 0);
        }
        branch_entry[i] += branch_count;
    }
}

pub fn distill_source(
    source_raw: &Value,
    sources: &mut HashMap<String, HashMap<String, SourceData>>,
    test_name: &str,
) {
    let source_name = source_raw["file_abs"].as_str().unwrap().to_string();

    // Ensure the source and test_name entry exists
    if !sources.contains_key(&source_name) {
        let mut test_entry = HashMap::new();
        let source_data = SourceData {
            functions: HashMap::new(),
            branches: HashMap::new(),
            lines: HashMap::new(),
        };
        test_entry.insert(test_name.to_string(), source_data);
        sources.insert(source_name.clone(), test_entry);
    }

    let test_entry = sources.get_mut(&source_name).unwrap();
    let source_data = test_entry.get_mut(test_name).unwrap();

    // Handle function data
    for function in source_raw["functions"].as_array().unwrap() {
        distill_function(function, &mut source_data.functions);  // Correct type
    }

    // Handle line and branch data
    for line in source_raw["lines"].as_array().unwrap() {
        distill_line(line, &mut source_data.lines, &mut source_data.branches);
    }
}

#[derive(Debug)]
pub struct SourceData {
    pub functions: HashMap<String, HashMap<String, i64>>,
    pub branches: HashMap<i64, Vec<i64>>,
    pub lines: HashMap<i64, i64>,
}
