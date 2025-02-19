use regex::Regex;
use rusqlite::params;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use crate::remover_config::Config;

const DEBUG: bool = false;

pub struct Remover {
    config: Config,
}

struct FunctionRange {
    name: String,
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

impl Remover {
    pub fn new(config_path: PathBuf) -> Self {
        let mut config_buf = String::new();
        File::open(config_path)
            .expect("Could not open config file!")
            .read_to_string(&mut config_buf)
            .expect("Could not read config file");
        let config = toml::from_str(&config_buf).unwrap();
        Remover { config }
    }

    pub fn remove(&self, no_change: bool) -> Result<(), Box<dyn std::error::Error>> {
        let rarely_used = self.get_rarely_used_lines()?;

        for (file, line_ranges) in rarely_used {
            // println!(
            //     "\n\nReplacing lines in file: {}",
            //     file.display().to_string()
            // );
            // println!("Lines to be replaced: {:?}", line_ranges);

            let imports = self.config.get_imports();
            let replacement = self.config.get_placeholder();
            self.replace_lines_in_file(&file, &replacement, &imports, &line_ranges, no_change)?;
        }

        Ok(())
    }

    fn get_rarely_used_lines(
        &self,
    ) -> Result<Vec<(PathBuf, Vec<FunctionRange>)>, Box<dyn std::error::Error>> {
        let conn = self.config.connect_to_db()?;
        let table_name = self.config.get_table_name()?;
        println!("[INFO] Table name: {}", table_name);

        let stmt = format!(
            "SELECT s.path, f.name, f.start_line, f.start_col, f.end_line, f.end_col
                FROM \"functions\" AS f
                JOIN \"sources\" AS s ON s.id = f.source_id
                JOIN \"{}\" AS u ON f.id = u.func_id
                WHERE u.use_function = 0
                ORDER BY s.path, f.start_line",
            table_name
        );
        let mut stmt = conn.prepare(&stmt)?;
        let rows = stmt.query_map(params![], |row| {
            let path: String = row.get(0)?;
            let name: String = row.get(1)?;
            let start_line: usize = row.get(2)?;
            let start_col: usize = row.get(3)?;
            let end_line: usize = row.get(4)?;
            let end_col: usize = row.get(5)?;

            Ok((path, name, start_line, start_col, end_line, end_col))
        })?;

        // Keep track of some statistics
        let mut total_func_count = 0;
        let mut remove_func_count = 0;

        // Aggregate query results into file_map, which groups the functions by files
        let file_map: HashMap<PathBuf, Vec<FunctionRange>> = {
            let mut result_map: HashMap<PathBuf, Vec<FunctionRange>> = HashMap::new();
            let mut curr_path: PathBuf = PathBuf::new();
            let mut curr_funcs: Vec<FunctionRange> = Vec::new();
            for row in rows {
                if let Ok((path, name, start_line, start_col, end_line, end_col)) = row {
                    total_func_count += 1;
                    let path = PathBuf::from(path);
                    if curr_funcs.is_empty() {
                        curr_path = path.clone();
                    }

                    if curr_path != path {
                        result_map.insert(curr_path, curr_funcs);
                        curr_funcs = Vec::new();
                        curr_path = PathBuf::new();
                    } else {
                        curr_funcs.push(FunctionRange {
                            name,
                            start_line,
                            start_col,
                            end_line,
                            end_col,
                        })
                    }
                }
            }
            if curr_funcs.len() > 0 {
                result_map.insert(curr_path, curr_funcs);
            }

            result_map
        };

        let mut result = vec![];

        for (path, functions) in file_map.iter() {
            // if !path.ends_with("src/theory/arrays/theory_arrays_rewriter.cpp") {
            //     continue;
            // }
            // if !path.ends_with("src/theory/strings/solver_state.cpp") {
            //     continue;
            // }
            // if !path.ends_with("theory/quantifiers/cegqi/ceg_bv_instantiator.cpp") {
            //     continue;
            // }
            // if !path.ends_with("src/theory/arith/linear/theory_arith_private.cpp") {
            //     continue;
            // }
            // if !path.ends_with("src/theory/quantifiers/first_order_model.cpp") {
            //     continue;
            // }

            let path = path.clone();
            let original_path = path.clone();
            let path = self.config.replace_path_prefix(path);

            if self.config.ignore_path_prefix(&original_path)
                || self.config.ignore_path_prefix(&path)
            {
                println!("[IGNORE] Ignoring file due to path rule!");
                continue;
            }

            let input_file = File::open(path.clone());
            if input_file.is_err() {
                println!("[ERROR] Couldn't open source code file!");
                continue;
            }
            let reader = BufReader::new(input_file?);

            let mut line_offset = 0;
            let mut line_no = 0;
            let mut depth: i64 = 0;
            let mut namespace_prefix = vec![];

            // function body detection values
            let mut body_chance: bool = false;
            let mut entered_body: bool = false;
            let mut prev_entered_body: bool = false;
            let mut is_inline: bool = false;
            let mut was_inline: bool = false;
            let mut in_comment: bool = false;
            let mut in_init_list: bool = false;
            let mut func_start_offset: usize = 0;

            // function specific values
            let mut func_depth: i64 = 0;
            let mut func_name: Option<String> = None;
            let mut func_start: usize = 0;
            let mut func_start_col: usize = 0;
            let mut func_end: usize;
            let mut func_end_col: usize;

            // Regex used to extract the function name.
            // A function name can only contain alphanumeric characters and underscores
            // Additionally it can not start with a number (we also allow ~, for destructors)
            // We also allow this repeated over and joined by :: for classes
            let func_name_regex = Regex::new(r"(([a-zA-Z_~]\w*::)?[a-zA-Z_~]\w*)\(").unwrap();

            // Try to parse all functions manually and make them accessible via range and via name
            let mut funcs_by_name = HashMap::new();
            let mut funcs_by_lines = HashMap::new();

            for line in reader.lines() {
                let mut line = line?;

                // Ignore inline functions and do not include them in our line number count
                if line.starts_with("inline ") {
                    is_inline = true;
                }
                if !is_inline && !was_inline {
                    line_no += 1;
                } else {
                    line_offset += 1;
                }
                // let line_no = line_no - 1;
                //

                if let Some(namespace) = line
                    .trim()
                    .strip_prefix("namespace")
                    .and_then(|l| l.strip_suffix("{"))
                {
                    namespace_prefix.push((depth, namespace.trim().to_string()));
                }

                if DEBUG {
                    println!(
                        "{} ({}) [Brackets: {} - {}] {{func: {}, fc: {}, il: {}; {}}}: {}",
                        if !is_inline && !was_inline {
                            "      "
                        } else {
                            "Inline"
                        },
                        line_no,
                        depth,
                        func_depth,
                        entered_body,
                        body_chance,
                        in_init_list,
                        func_start_offset,
                        line
                    );
                }

                // Skip comments, as we do not want to count them
                if line.trim_start().starts_with("//") {
                    continue;
                } else if line.trim_start().starts_with("/*") {
                    in_comment = true;
                }
                if in_comment {
                    // In case of a multiline comment only crop out the comment part
                    if let Some(start) = line.find("*/") {
                        in_comment = false;
                        line = line[start..].to_string();
                    } else {
                        continue;
                    }
                }

                // TODO: Somehow check the function signature for correctness of the function
                // body

                let open_count = line.chars().filter(|&c| c == '{').count();
                let close_count = line.chars().filter(|&c| c == '}').count();

                depth += (open_count as i64) - (close_count as i64);
                if entered_body {
                    func_depth += (open_count as i64) - (close_count as i64);
                }

                namespace_prefix = namespace_prefix
                    .into_iter()
                    .filter(|(d, _)| d <= &depth)
                    .collect();

                // Only start counting the brackets once we have entered the body
                entered_body = entered_body && func_depth > 0;
                if !entered_body {
                    let chars = ['{', '}', '(', ')'];
                    if !in_init_list {
                        in_init_list = !entered_body && body_chance && line.contains(": ");
                        if !in_init_list {
                            if let Some(capture) = func_name_regex.captures(line.as_str()) {
                                // println!("{:?}", capture);
                                func_name = Some(capture[1].to_string());
                            }
                        }
                    }
                    entered_body = line.chars().any(|c| chars.contains(&c)) && {
                        let str: String = line.chars().filter(|c| chars.contains(c)).collect();
                        let res = (body_chance && str == "{") || str == "(){";
                        if !in_init_list {
                            body_chance = str == "()"
                                || str == "("
                                || str == ")"
                                || (body_chance && line.trim_end().ends_with(","))
                                || (body_chance && str == ")");
                        }
                        res
                    };
                    if entered_body {
                        in_init_list = false;
                        body_chance = false;
                        func_start = line_no;
                        func_start_col = line.find("{").unwrap();
                        func_depth = (open_count as i64) - (close_count as i64);
                    }
                }

                let func_ended = {
                    if !is_inline {
                        if !entered_body
                            && open_count > 0
                            && line
                                .chars()
                                .filter(|c| ['{', '}'].contains(c))
                                .collect::<String>()
                                .ends_with("{}")
                        {
                            func_start = line_no;
                            func_start_col = line.find("{").unwrap();
                            true
                        } else {
                            entered_body != prev_entered_body && func_depth <= 0
                        }
                    } else {
                        false
                    }
                };
                // We just left a function body and therefore found all information about a
                // function!
                if func_ended {
                    func_end = line_no;
                    func_end_col = line.rfind("}").unwrap_or(0);
                    if DEBUG {
                        println!(
                            "Found function '{}' from line {} ({}) to line {}",
                            func_name.clone().unwrap_or("N/A".to_string()),
                            func_start,
                            func_start - func_start_offset,
                            func_end
                        );
                    }

                    if let Some(func_name) = func_name {
                        let mut func_name_prefix: String = String::new();
                        for (_, n) in namespace_prefix.iter() {
                            func_name_prefix.push_str((n.to_string() + "::").as_str());
                        }
                        let full_func_name = func_name_prefix.clone() + func_name.clone().as_str();
                        funcs_by_name.insert(
                            full_func_name.clone(),
                            (
                                line_offset + func_start,
                                line_offset + func_end,
                                func_start_col,
                                func_end_col,
                            ),
                        );
                        // Additional entry as gcov sometimes does not use the last namespace id
                        if let Some((_, func_name)) = func_name.rsplit_once("::") {
                            let reduced_func_name = format!("{}{}", func_name_prefix, func_name);
                            funcs_by_name.insert(
                                reduced_func_name.clone(),
                                (
                                    line_offset + func_start,
                                    line_offset + func_end,
                                    func_start_col,
                                    func_end_col,
                                ),
                            );
                        }
                        // Additional entry as gcov sometimes does not use the class identifier
                        let func_parts: Vec<&str> = func_name_prefix.rsplitn(3, "::").collect();
                        if func_parts.len() == 3 {
                            let reduced_func_name = format!("{}::{}", func_parts[2], func_name);
                            funcs_by_name.insert(
                                reduced_func_name.clone(),
                                (
                                    line_offset + func_start,
                                    line_offset + func_end,
                                    func_start_col,
                                    func_end_col,
                                ),
                            );
                        }
                    }
                    funcs_by_lines.insert(
                        (func_start - func_start_offset, func_end),
                        (
                            line_offset + func_start,
                            line_offset + func_end,
                            func_start_col,
                            func_end_col,
                        ),
                    );

                    func_name = None;
                    func_depth = 0;
                }

                if body_chance {
                    func_start_offset += 1;
                } else if !entered_body {
                    func_start_offset = 0
                }

                prev_entered_body = entered_body;
                was_inline = is_inline;
                is_inline &= func_depth > 0;
            }

            // Now try to find corresponding matching ranges in our functions
            let mut file_res = vec![];
            for function in functions {
                let ignore_func = |p| {
                    self.config.ignore_path(
                        p,
                        &function.name,
                        &function.start_line,
                        &function.end_line,
                    )
                };
                if ignore_func(&original_path) || ignore_func(&path) {
                    println!("[IGNORE]\tIgnoring function due to path rules!");
                    continue;
                }

                let start_line;
                let start_col;
                let end_line;
                let end_col;

                // FIXME: Name detection is somewhat broken, due to sometimes the signature being
                // reported by gcov being wrong...

                // Detected function start and end by name
                let temp_name = function.name.split_once("(").unwrap().0;
                // println!("Checking for function {}!", temp_name);
                // println!("Keys: {:?}", funcs_by_name.keys());
                if let Some(&(start, end, start_c, end_c)) = funcs_by_name.get(temp_name) {
                    // println!("Found function {} by name!", function.name);
                    start_line = start;
                    start_col = start_c;
                    end_line = end;
                    end_col = end_c;
                } else
                // Detected function start and end by 'gcov lines'
                if let Some(&(start, end, start_c, end_c)) =
                    funcs_by_lines.get(&(function.start_line, function.end_line))
                {
                    // println!("Found function {} by lines!", function.name);
                    start_line = start;
                    start_col = start_c;
                    end_line = end;
                    end_col = end_c;
                } else {
                    println!(
                        "[MISS] Could not find appropiate match for '{}'\n\t\t (exp start: {}, end: {}, file: {})",
                        function.name,
                        function.start_line,
                        function.end_line,
                        path.display().to_string()
                    );
                    if DEBUG {
                        println!(
                        "[MISS-INFO] Tried finding: {}\n[MISS-INFO] Available function keys: {:?}",
                        temp_name,
                        funcs_by_name.keys()
                    );
                    }
                    continue;
                }

                // let line_diff = function.end_line - function.start_line;
                // if line_diff > 2 {
                remove_func_count += 1;
                file_res.push(FunctionRange {
                    name: function.name.clone(),
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                });
                // } else {
                //     println!(
                //         "[SKIP]\tSkipping function '{}' due to small line change\n\t\t (start: {}, end: {}, file: {})",
                //         function.name,
                //         function.start_line, function.end_line,
                //         path.display().to_string()
                //     );
                // }
            }

            result.push((path, file_res));
        }

        println!(
            "[STATS]\tRemoving {} of {} functions",
            remove_func_count, total_func_count
        );
        Ok(result)
    }

    fn replace_lines_in_file(
        &self,
        file_path: &PathBuf,
        replacement_str: &str,
        additional_imports: &Vec<String>,
        skip_ranges: &Vec<FunctionRange>,
        no_change: bool,
    ) -> io::Result<()> {
        if skip_ranges.len() == 0 {
            return Ok(());
        }

        let input_file = File::open(file_path)?;
        let reader = BufReader::new(input_file);

        // Work in temporary file
        let temp_file_path = format!("{}.tmp", file_path.to_str().unwrap());
        let temp_file = File::create(&temp_file_path)?;
        let mut writer = io::BufWriter::new(temp_file);

        // Prepend required imports
        for line in additional_imports {
            if no_change {
                println!("{}", line);
            } else {
                writeln!(writer, "{}", line)?;
            }
        }

        let mut skip_iter = skip_ranges.iter();
        let mut skip_range = skip_iter.next();

        for (line_no, line) in reader.lines().enumerate() {
            let line_no = line_no + 1;
            let line = line?;
            if let Some(frange) = skip_range {
                let (name, start, end, start_col, end_col) = (
                    frange.name.clone(),
                    frange.start_line,
                    frange.end_line,
                    frange.start_col,
                    frange.end_col,
                );
                if line_no < start || line_no > end {
                    // Write lines not in the specified range to the temporary file
                    if no_change {
                        println!("{}", line);
                    } else {
                        writeln!(writer, "{}", line)?;
                    }
                } else {
                    if line_no == start {
                        if no_change {
                            print!("{}{{", line[..start_col].to_string());
                        } else {
                            write!(writer, "{}{{", line[..start_col].to_string())?;
                        }
                    }
                    // We reached the end of the current skip range
                    if line_no == end {
                        // Replace placeholders
                        let replacement_str = replacement_str
                            .replace("{func_name}", &name)
                            .replace("{file_name}", &file_path.display().to_string());
                        let remainder = if line.len() > end_col {
                            &line[end_col + 1..]
                        } else {
                            ""
                        };
                        // Insert our "dummy code" and the remainder
                        if no_change {
                            print!("{}}}{}", replacement_str, remainder.to_string());
                        } else {
                            write!(writer, "{}}}{}", replacement_str, remainder.to_string())?;
                        }

                        // Fetch the next skip range
                        skip_range = skip_iter.next();
                    }
                }
            } else {
                if no_change {
                    println!("{}", line);
                } else {
                    writeln!(writer, "{}", line)?;
                }
            }
        }

        if !no_change {
            // Replace the original file with the temporary file
            fs::rename(temp_file_path, file_path)?;
        } else {
            fs::remove_file(temp_file_path)?;
        }

        Ok(())
    }
}
