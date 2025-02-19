use bitvec::prelude::*;
use ordered_float::OrderedFloat;
use plotters::prelude::*;
use rayon::prelude::*;
use rusqlite::{params, Connection, OpenFlags};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

const DEBUG: bool = false;

pub struct Analyzer {
    db_path: String,
    path_rewrite: Option<(String, String)>,

    line_deviations: Option<HashMap<(String, String), (i64, i64)>>,
}

impl Analyzer {
    pub fn new(db_path: String, path_rewrite: Option<Vec<String>>) -> Self {
        Analyzer {
            db_path,
            line_deviations: None,
            path_rewrite: path_rewrite.map(|v| (v[0].to_owned(), v[1].to_owned())),
        }
    }

    pub fn get_line_deviations(
        &mut self,
    ) -> Result<&HashMap<(String, String), (i64, i64)>, Box<dyn std::error::Error>> {
        if self.line_deviations.is_none() {
            let res = self.check_func_range_correctness()?;
            self.line_deviations = Some(res);
        }

        Ok(self.line_deviations.as_ref().unwrap())
    }

    pub fn analyze_smallest_benches(&mut self, p: f64) -> Result<(), Box<dyn std::error::Error>> {
        if p <= 0.0 || p > 1.00 {
            return Err(Box::from("Expected a p value in range (0,1]"));
        }

        let table_name = format!(
            "optimization_result_p0_{}",
            (OrderedFloat(p) * OrderedFloat(10000.0)).round() as u32
        );

        let fuid_bench_map: HashMap<String, PathBuf> = self.check_min_benches(&table_name)?;
        println!(
            "Mapping of benchmarks requiring removed functions for p={} (Total: {}):",
            p,
            fuid_bench_map.len()
        );
        for (fuid, bench) in fuid_bench_map.iter() {
            println!(
                "\tFunction '{}' needs '{}'",
                fuid,
                bench.display().to_string()
            );
        }

        let bench_set: HashSet<&PathBuf> = HashSet::from_iter(fuid_bench_map.values());
        println!(
            "Set of minimal benchmark examples removed for p={} (Total: {}):",
            p,
            bench_set.len()
        );
        for bench in bench_set {
            println!("\t{}", bench.display().to_string());
        }

        Ok(())
    }

    pub fn analyze_line_deviations(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let line_deviations = self.get_line_deviations()?;

        let mut total_count = 0;
        let mut start_dev_count = 0;
        let mut end_dev_count = 0;
        let mut avg_start = 0;
        let mut avg_end = 0;
        let mut max_start = i64::MIN;
        let mut max_end = i64::MIN;
        let mut min_start = i64::MAX;
        let mut min_end = i64::MAX;
        for ((_path, _name), &(start_dev, end_dev)) in line_deviations {
            total_count += 1;

            if end_dev != 0 {
                avg_end += end_dev;
                end_dev_count += 1;
            }
            if start_dev != 0 {
                avg_start += start_dev;
                start_dev_count += 1;
            }
            max_start = max_start.max(start_dev);
            max_end = max_end.max(end_dev);
            min_start = min_start.min(start_dev);
            min_end = min_end.min(end_dev);
        }
        let avg_start = avg_start as f64 / total_count as f64;
        let avg_end = avg_end as f64 / total_count as f64;

        println!(
            "Functions with wrong start line numbers: {} of {}",
            start_dev_count, total_count
        );
        println!(
            "Functions with wrong end line numbers: {} of {}",
            end_dev_count, total_count
        );
        println!(
            "Average Deviation:\n\t Start: {}\n\t End: {}",
            avg_start, avg_end
        );
        println!(
            "Max Deviation:\n\t Start: {}\n\t End: {}",
            max_start, max_end
        );
        println!(
            "Min Deviation:\n\t Start: {}\n\t End: {}",
            min_start, min_end
        );

        Ok(())
    }

    pub fn visualize_line_deviations(
        &mut self,
        dst_path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let line_deviations = self.get_line_deviations()?;
        let mut start_deviations = HashMap::new();
        let mut end_deviations = HashMap::new();
        line_deviations.values().for_each(|(start_dev, end_dev)| {
            if let Some(v) = start_deviations.get_mut(start_dev) {
                *v += 1;
            } else {
                start_deviations.insert(start_dev, 1);
            }
            if let Some(v) = end_deviations.get_mut(end_dev) {
                *v += 1;
            } else {
                end_deviations.insert(end_dev, 1);
            }
        });

        let root = BitMapBackend::new(dst_path, (1920, 1080)).into_drawing_area();

        root.fill(&WHITE)?;
        let (left, right) = root.split_horizontally(960);

        for (title, side, deviations) in [
            ("Start Lines", left, start_deviations),
            ("End Lines", right, end_deviations),
        ] {
            let mut chart = ChartBuilder::on(&side)
                .x_label_area_size(40)
                .y_label_area_size(40)
                .margin(10)
                .caption(title, ("sans-serif", 50.0))
                .build_cartesian_2d(
                    (**deviations.keys().min().unwrap() as i64)
                        ..(**deviations.keys().max().unwrap() as i64) / 10,
                    (*deviations.values().min().unwrap() as i64)
                        ..(*deviations.values().max().unwrap() as i64),
                )?;

            chart
                .configure_mesh()
                .disable_x_mesh()
                .bold_line_style(WHITE.mix(0.3))
                .y_desc("No. Functions")
                .x_desc("Deviation")
                // Shift the labels it actually aligns with the bar
                .x_label_offset(10)
                .axis_desc_style(("sans-serif", 15))
                .draw()?;

            chart.draw_series(
                Histogram::vertical(&chart)
                    .style(RED.mix(0.5).filled())
                    .data(deviations.iter().map(|(&&k, &v)| (k, v))),
            )?;
        }

        // To avoid the IO failure being ignored silently, we manually call the present function
        root.present().expect("Unable to write result to file, please make sure 'plotters-doc-data' dir exists under current dir");
        println!("Result has been saved to {}", dst_path);

        Ok(())
    }

    fn count_benchmark_tokens(bench_path: &PathBuf) -> Result<usize, Box<dyn std::error::Error>> {
        let mut token_count: usize = 0;

        let file = fs::read(bench_path)?;
        // Ignore the content of set-info to not dilute the word count
        let mut in_info = false;
        for line in file.lines() {
            let line = line?;
            if line.trim_start().starts_with("(set-info ") {
                in_info = true;
            }
            if !in_info {
                token_count += line.split_whitespace().count();
            }
            if in_info && line.trim_end().ends_with(")") {
                in_info = false;
            }
        }

        Ok(token_count)
    }

    fn check_min_benches(
        &self,
        table_name: &String,
    ) -> Result<HashMap<String, PathBuf>, Box<dyn std::error::Error>> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        println!("Retrieving token count for each benchmark...");
        let stmt = "SELECT id, path FROM \"benchmarks\" ORDER BY id";
        let mut stmt = conn.prepare(&stmt)?;
        let rows = stmt.query_map(params![], |row| {
            let id: usize = row.get(0)?;
            let mut path: String = row.get(1)?;
            if let Some((old, new)) = self.path_rewrite.clone() {
                let old_path = path.clone();
                path = path.replace(old.as_str(), new.as_str());
                if DEBUG {
                    println!("Changing path '{}' to '{}'", old_path, path);
                }
            }
            let path = PathBuf::from(path);

            Ok((id, path))
        })?;
        let mut ires = vec![];
        for row in rows {
            if let Ok((id, path)) = row {
                ires.push((id, path));
            }
        }
        let benchmark_pairs: Vec<(usize, (PathBuf, usize))> = ires
            .par_iter()
            .map(|(id, path)| {
                // println!("Analyzing Path: {}", path.display().to_string());
                let token_count = Self::count_benchmark_tokens(&path);
                if token_count.is_err() {
                    println!(
                        "Error during file {} encountered. Skipping...",
                        path.display().to_string()
                    );
                }
                let token_count = token_count.unwrap_or(usize::MAX);
                (*id, (path.clone(), token_count))
            })
            .collect();
        let mut benchmarks: HashMap<usize, (PathBuf, usize)> = HashMap::new();
        for (k, v) in benchmark_pairs {
            benchmarks.insert(k, v);
        }

        println!("Finding smallest benchmark for each unused function...");
        let stmt = format!(
            "SELECT f.id, f.name, b.data
            FROM \"functions\" AS f
            JOIN \"function_bitvecs\" AS b ON b.function_id = f.id
            JOIN \"{}\" AS r ON r.func_id = f.id
            WHERE r.use_function = 0
            ORDER BY f.id",
            table_name
        );
        let mut stmt = conn.prepare(&stmt)?;
        let rows = stmt.query_map(params![], |row| {
            let id: usize = row.get(0)?;
            let name: String = row.get(1)?;
            let fuid = format!("{}:{}", id, name);

            let slice: &[u8] = row.get_ref(2)?.as_blob()?;
            let bitvec: BitVec<u8, Msb0> = BitVec::from_slice(slice);
            let smallest_bench: PathBuf = {
                let mut smallest_token_count = usize::MAX;
                let mut smallest_path: PathBuf = PathBuf::new();
                for (bench_id, bench_required) in bitvec.iter().enumerate() {
                    if !bench_required {
                        continue;
                    }
                    let bench_id = bench_id + 1;
                    let (bench_path, bench_token_count) = benchmarks.get(&bench_id).unwrap();
                    if bench_token_count < &smallest_token_count {
                        smallest_token_count = *bench_token_count;
                        smallest_path = bench_path.clone();
                    }
                }
                smallest_path
            };

            Ok((fuid, smallest_bench))
        })?;
        let mut min_benches: HashMap<String, PathBuf> = HashMap::new();
        for row in rows {
            if let Ok((fuid, smallest_bench)) = row {
                min_benches.insert(fuid, smallest_bench);
            }
        }
        Ok(min_benches)
    }

    fn check_func_range_correctness(
        &self,
    ) -> Result<HashMap<(String, String), (i64, i64)>, Box<dyn std::error::Error>> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let stmt = format!(
            "SELECT s.path, f.name, f.start_line, f.start_col, f.end_line, f.end_col
    FROM \"functions\" AS f
    JOIN \"sources\" AS s ON s.id = f.source_id
    ORDER BY s.path, f.start_line",
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
        let mut func_line_deviations: HashMap<(String, String), (i64, i64)> = HashMap::new();
        for row in rows {
            if let Ok((path, name, start_line, _start_col, end_line, _end_col)) = row {
                let mut path = path;
                if let Some((old, new)) = self.path_rewrite.clone() {
                    let old_path = path.clone();
                    path = path.replace(old.as_str(), new.as_str());
                    if DEBUG {
                        println!("Changing path '{}' to '{}'", old_path, path);
                    }
                }

                let input = if let Ok(v) = File::open(&path) {
                    v
                } else {
                    println!("Could not open source file '{}'", path);
                    continue;
                };
                let reader = BufReader::new(input);

                let mut real_start_line = start_line;
                let mut real_end_line = end_line;

                let mut func_body_entered = false;
                for (line_no, line) in reader.lines().enumerate().skip(start_line) {
                    let line_no = line_no + 1;
                    let line = line?;

                    // If inside the specified write lines until the body block has started
                    if !func_body_entered {
                        func_body_entered = line.ends_with("{");
                        if func_body_entered {
                            real_start_line = line_no;
                        }
                    }

                    if func_body_entered {
                        if line_no <= end_line - 2 {
                            continue;
                        } else if line.ends_with("}") {
                            real_end_line = line_no;
                            break;
                        }
                    }
                }

                let start_deviation = (real_start_line as i64) - (start_line as i64);
                let end_deviation = (real_end_line as i64) - (end_line as i64);
                func_line_deviations.insert((path, name), (start_deviation, end_deviation));
            }
        }
        Ok(func_line_deviations)
    }
}
