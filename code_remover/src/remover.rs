use rusqlite::{params, Connection, OpenFlags};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};

pub struct Remover {
    db_path: String,
}

impl Remover {
    pub fn new(db_path: String) -> Self {
        Remover { db_path }
    }

    pub fn remove(
        &self,
        table_name: &str,
        no_change: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let rarely_used = self.get_rarely_used_lines(table_name)?;

        for (file, line_ranges) in rarely_used {
            // if !file.ends_with("sat_solver_types.h") {
            //     continue;
            // }

            println!("\n\nReplacing lines in file: {}", file);
            println!("Lines to be replaced: {:?}", line_ranges);
            self.replace_lines_in_file( &file, "std::cout << \"Unsupported feature\" << std::endl; exit(1000); __builtin_unreachable();", &vec!["#include <iostream>".to_string()], &line_ranges, no_change)?;
        }

        Ok(())
    }

    fn get_rarely_used_lines(
        &self,
        table_name: &str,
    ) -> Result<Vec<(String, Vec<(usize, usize)>)>, Box<dyn std::error::Error>> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let stmt = format!(
            "SELECT s.path, f.name, f.start_line, f.start_col, f.end_line, f.end_col
    FROM \"functions\" AS f
    JOIN \"sources\" AS s ON s.id = f.source_id
    JOIN \"{}\" AS u ON f.id = u.func_id
    WHERE u.use_function = 1
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
        let mut prev_src: String = "".to_string();
        for row in rows {
            if let Ok((path, name, start_line, _start_col, end_line, _end_col)) = row {
                // FIXME: Filter these out ahead of time
                if path.starts_with("/local/home/jordanpa/cvc5-repo/build/") {
                    continue;
                }
                // FIXME: Find a better way to filter out edge cases
                if path == "/local/home/jordanpa/cvc5-repo/src/api/cpp/cvc5.cpp" {
                    if start_line >= 7743 && end_line <= 7839 {
                        continue;
                    }
                }
                // FIXME: This is for local testing only
                let path = path.replace("/local/home/jordanpa/", "../../");
                if prev_src != path {
                    result.push((prev_src, source_result));
                    source_result = vec![];
                    prev_src = path;
                }

                // FIXME: Detect Constructors in a better way
                // FIXME: Detect deconstructors in a better way
                if name.contains("::~") {
                    continue;
                }
                if end_line - start_line >= 1 {
                    source_result.push((start_line, end_line - 1));
                }
            }
        }
        // The first entry is always empty
        result.remove(0);
        Ok(result)
    }

    pub fn replace_lines_in_file(
        &self,
        file_path: &str,
        replacement_str: &str,
        additional_imports: &Vec<String>,
        skip_ranges: &Vec<(usize, usize)>,
        no_change: bool,
    ) -> io::Result<()> {
        let input_file = File::open(file_path)?;
        let reader = BufReader::new(input_file);

        // Work in temporary file
        let temp_file_path = format!("{}.tmp", file_path);
        let temp_file = File::create(&temp_file_path)?;
        let mut writer = io::BufWriter::new(temp_file);

        if skip_ranges.len() == 0 {
            return Ok(());
        }

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
            if let Some((start, end)) = skip_range {
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
