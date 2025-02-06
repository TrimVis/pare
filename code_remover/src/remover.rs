use rusqlite::params;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use crate::remover_config::Config;

pub struct Remover {
    config: Config,
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
            // if !file.ends_with("sat_solver_types.h") {
            //     continue;
            // }

            // println!(
            //     "\n\nReplacing lines in file: {}",
            //     file.display().to_string()
            // );
            // println!("Lines to be replaced: {:?}", line_ranges);

            let imports = vec!["#include <iostream>".to_string()];
            let replacement = format!(
                "std::cout << \"Unsupported feature '{}': '{{}}'\" << std::endl; exit(1000); __builtin_unreachable();", 
                file.display().to_string()
            );
            self.replace_lines_in_file(&file, &replacement, &imports, &line_ranges, no_change)?;
        }

        Ok(())
    }

    fn get_rarely_used_lines(
        &self,
    ) -> Result<Vec<(PathBuf, Vec<(String, usize, usize)>)>, Box<dyn std::error::Error>> {
        let conn = self.config.connect_to_db()?;
        let table_name = self.config.get_table_name()?;
        println!("Table name: {}", table_name);

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
        let mut result = vec![];
        let mut source_result = vec![];
        let mut prev_src: PathBuf = PathBuf::new();

        let mut count = 0;
        let mut total_count = 0;

        for row in rows {
            if let Ok((path, name, start_line, _start_col, end_line, _end_col)) = row {
                let (exp_start_line, exp_end_line) = (start_line, end_line);

                total_count += 1;
                let mut end_line = end_line;
                let path = PathBuf::from(path);
                if self
                    .config
                    .ignore_path(&path, &name, &start_line, &end_line)
                {
                    println!("Ignoring file due to rules with original path!");
                    continue;
                }
                let prev_path = path.clone();
                let path = self.config.replace_path_prefix(path);
                if prev_path != path
                    && self
                        .config
                        .ignore_path(&path, &name, &start_line, &end_line)
                {
                    println!("Ignoring file due to rules with rewritten path!");
                    continue;
                }

                let input_file = File::open(path.clone());
                if input_file.is_err() {
                    println!("Couldn't open source code file!");
                    continue;
                }
                let reader = BufReader::new(input_file?);
                let mut bracket_counter: i64 = 0;
                let mut entered_body: bool = false;
                let mut in_comment: bool = false;
                for (line_no, line) in reader.lines().enumerate().skip(start_line - 1) {
                    let line = line?;
                    // TODO: Somehow check the function signature for correctness of the function
                    // body
                    // if line_no == start_line {
                    //     println!("Func: {}\nStartline: {}", name, line);
                    // }

                    // Skip comments
                    if line.trim_start().starts_with("//") {
                        continue;
                    } else if line.trim_start().starts_with("/*") {
                        in_comment = true;
                    }
                    if in_comment {
                        if line.contains("*/") {
                            in_comment = false;
                        }
                        continue;
                    }

                    let open_count = line.chars().filter(|&c| c == '{').count();
                    let close_count = line.chars().filter(|&c| c == '}').count();

                    // Only start counting the brackets once we have entered the body
                    if !entered_body {
                        entered_body = open_count > 0;
                    }
                    if entered_body {
                        bracket_counter += (open_count as i64) - (close_count as i64);
                        if bracket_counter <= 0 {
                            end_line = line_no;
                            break;
                        }
                    }
                }

                if prev_src != path {
                    if source_result.len() > 0 {
                        result.push((prev_src, source_result));
                    }
                    source_result = vec![];
                    prev_src = path.clone();
                }

                let line_diff = end_line as i64 - start_line as i64;
                if line_diff > 2 {
                    source_result.push((name, start_line, end_line));
                } else {
                    let exp_line_diff = exp_end_line as i64 - exp_start_line as i64;
                    println!(
                        "Ignoring function '{}' due to small line change (file: {})",
                        name,
                        path.display().to_string()
                    );
                    println!(
                        "Expected start line: {}; Expected end line: {}",
                        exp_start_line, exp_end_line,
                    );
                    println!(
                        "Found start line: {}; Found end line: {}",
                        start_line, end_line,
                    );
                    println!(
                        "Expected diff: {}; Calculated diff: {}",
                        exp_line_diff, line_diff
                    );
                }

                count += 1;
            }
        }
        // The first entry is always empty
        result.remove(0);

        println!("Removed: {}/{}", count, total_count);
        Ok(result)
    }

    pub fn replace_lines_in_file(
        &self,
        file_path: &PathBuf,
        replacement_str: &str,
        additional_imports: &Vec<String>,
        skip_ranges: &Vec<(String, usize, usize)>,
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

        let mut func_body_entered = false;
        let mut skip_iter = skip_ranges.iter();
        let mut skip_range = skip_iter.next();

        for (line_no, line) in reader.lines().enumerate() {
            let line_no = line_no + 1;
            let line = line?;
            if let Some((name, start, end)) = skip_range {
                if line_no < *start || line_no > *end {
                    // Write lines not in the specified range to the temporary file
                    if no_change {
                        println!("{}", line);
                    } else {
                        writeln!(writer, "{}", line)?;
                    }
                } else {
                    // If inside the specified write lines until the body block has started
                    if !func_body_entered {
                        if no_change {
                            println!("{}", line);
                        } else {
                            writeln!(writer, "{}", line)?;
                        }
                        func_body_entered = line.ends_with("{");
                    }

                    // We reached the end of the current skip range
                    if *end <= line_no {
                        if func_body_entered {
                            let replacement_str = replacement_str.replacen("{}", name, 1);
                            // Insert our "dummy code"
                            if no_change {
                                println!("{}", replacement_str);
                            } else {
                                writeln!(writer, "{}", replacement_str)?;
                            }
                        }

                        // Fetch the next skip range
                        skip_range = skip_iter.next();
                        func_body_entered = false;
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
