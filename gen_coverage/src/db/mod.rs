mod init;
use crate::args::{TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::runner::GcovRes;
use crate::types::{Benchmark, Cvc5BenchmarkRun};
use crate::{ResultT, ARGS};

use itertools::Itertools;
use log::info;
use rusqlite::{params, Connection, OpenFlags};
use std::collections::HashMap;
use std::path::PathBuf;

const MEMORY_CONN_URI: &str = ":memory:";
const INSERT_BATCH_SIZE: usize = 400;

pub struct DbWriter {
    conn: Connection,
}

impl DbWriter {
    pub fn new(init: bool) -> ResultT<Self> {
        let mut conn = Connection::open_with_flags(
            MEMORY_CONN_URI,
            OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        if init {
            info!("Configuring database...");
            init::prepare(&conn).expect("Issue during table preparation");
            info!("Creating tables...");
            init::create_tables(&conn).expect("Issue during table creation");
            info!("Populating config table...");
            init::populate_config(conn.transaction()?)
                .expect("Issue during config table population");
            info!("Populating benchmarks table...");
            init::populate_benchmarks(conn.transaction()?)
                .expect("Issue during benchmark table population");
        }

        Ok(DbWriter { conn })
    }

    pub fn write_to_disk(&self) -> ResultT<()> {
        let query = format!("VACUUM INTO '{}'", ARGS.result_db.display());
        self.conn.execute(&query, params![])?;
        Ok(())
    }

    pub fn get_all_benchmarks(&mut self) -> ResultT<Vec<Benchmark>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, prefix FROM \"benchmarks\"")?;
        let rows = stmt.query_map(params![], |row| {
            let pref: String = row.get(2)?;
            let path: String = row.get(1)?;
            Ok(Benchmark {
                id: row.get(0)?,
                path: PathBuf::from(path),
                prefix: if pref.len() > 0 {
                    Some(PathBuf::from(pref))
                } else {
                    None
                },
            })
        })?;
        let mut result = vec![];
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    pub fn add_cvc5_run_result(&mut self, run_result: Cvc5BenchmarkRun) -> ResultT<()> {
        let mut stmt_insert_cvc5result = self
            .conn
            .prepare_cached(
                "INSERT INTO \"result_benchmarks\" (
                bench_id,
                time_ms,
                exit_code,
                stdout,
                stderr
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .expect("Issue during benchmark status update query preparation");
        stmt_insert_cvc5result
            .execute(params![
                run_result.bench_id,
                run_result.time_ms,
                run_result.exit_code,
                run_result.stdout,
                run_result.stderr,
            ])
            .expect("Issue during cvc5 run result insertion!");
        Ok(())
    }

    pub fn add_gcov_measurement(&mut self, run_result: GcovRes) -> ResultT<()> {
        let tx = self.conn.transaction()?;
        // 1. Ensure all sources exist in DB & retrieve their ids
        {
            for chunk in &run_result.iter().chunks(INSERT_BATCH_SIZE) {
                let mut batch_query = String::new();
                for (file, _) in chunk {
                    batch_query.push_str(&format!(
                        "INSERT INTO \"sources\" ( path ) VALUES ( '{}' ) ON CONFLICT DO NOTHING;",
                        file
                    ));
                }
                tx.execute_batch(&batch_query)?;
            }
        }

        let mut srcid_file_map: HashMap<Box<String>, u64>;
        {
            let mut stmt = tx.prepare_cached("SELECT id, path FROM \"sources\"")?;
            let rows = stmt.query_map(params![], |row| {
                let id: u64 = row.get(0)?;
                let file: String = row.get(1)?;
                Ok((file, id))
            })?;
            srcid_file_map = HashMap::with_capacity(rows.size_hint().0);
            for row in rows {
                let (file, id) = row?;
                srcid_file_map.insert(Box::from(file), id);
            }
        }

        // 2. Track usage data of all (used) functions
        if TRACK_FUNCS.clone() {
            for (file, (funcs, _, _)) in &run_result {
                let sid = srcid_file_map.get(file).unwrap();
                for chunk in &funcs
                    .values()
                    .filter(|f| f.borrow().usage > 0)
                    .chunks(INSERT_BATCH_SIZE)
                {
                    let mut batch_query = String::new();
                    for func in chunk {
                        // NOTE: As the function name also contains the parameter types,
                        // overloading kind of breaks the names and they should be used with care
                        // ON CONFLICT (source_id, name) DO UPDATE
                        let func = func.borrow();
                        batch_query.push_str(&format!(
                            "INSERT INTO \"functions\" (
                            source_id,
                            name,
                            start_line,
                            start_col,
                            end_line,
                            end_col,
                            benchmark_usage_count
                        ) VALUES ('{}', '{}', {}, {}, {}, {}, {}) 
                        ON CONFLICT (source_id, start_line, start_col) DO UPDATE 
                        SET benchmark_usage_count = benchmark_usage_count + excluded.benchmark_usage_count;",
                            sid.to_string(),
                            func.name.to_string(),
                            func.start.line.to_string(),
                            func.start.col.to_string(),
                            func.end.line.to_string(),
                            func.end.col.to_string(),
                            func.usage
                        ));
                    }
                    tx.execute_batch(&batch_query)?;
                }
            }
        }

        // 2. Track usage data of all (used) lines
        if TRACK_LINES.clone() {
            for (file, (_, lines, _)) in &run_result {
                let sid = srcid_file_map.get(file).unwrap();
                for chunk in &lines
                    .values()
                    .filter(|l| l.borrow().usage > 0)
                    .chunks(INSERT_BATCH_SIZE)
                {
                    let mut batch_query = String::new();
                    for line in chunk {
                        let line = line.borrow();
                        batch_query.push_str(&format!(
                            "INSERT INTO \"lines\" (
                            source_id,
                            line_no,
                            benchmark_usage_count
                        ) VALUES ({}, {}, {})
                        ON CONFLICT (source_id, line_no) DO UPDATE 
                        SET benchmark_usage_count = benchmark_usage_count + excluded.benchmark_usage_count;",
                            *sid,
                            line.line_no,
                            line.usage
                        ));
                    }
                    tx.execute_batch(&batch_query)?;
                }
            }
        }

        if TRACK_BRANCHES.clone() {
            // TODO: Add support for branch tracking
            unimplemented!("Branch tracking not yet supported")
        }
        tx.commit()?;

        Ok(())
    }
}
