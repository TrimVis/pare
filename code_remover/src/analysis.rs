use plotters::prelude::*;
use rusqlite::{params, Connection, OpenFlags};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

pub struct Analyzer {
    db_path: String,
    line_deviations: Option<HashMap<(String, String), (i64, i64)>>,
}

impl Analyzer {
    pub fn new(db_path: String) -> Self {
        Analyzer {
            db_path,
            line_deviations: None,
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

    pub fn analyze(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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

    pub fn visualize(&mut self, dst_path: &str) -> Result<(), Box<dyn std::error::Error>> {
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
                // FIXME: Filter these out ahead of time
                if path.starts_with("/local/home/jordanpa/cvc5-repo/build/") {
                    continue;
                }

                // FIXME: Detect Constructors in a better way
                // FIXME: Detect destructors in a better way
                if name.contains("::~") {
                    continue;
                }

                // FIXME: This is for local testing only
                let path = path.replace("/local/home/jordanpa/", "../../");

                let input = File::open(&path)?;
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
