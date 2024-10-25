mod init;
use crate::args::{TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::runner::GcovRes;
use crate::types::{Benchmark, Cvc5BenchmarkRun, Status};
use crate::{ResultT, ARGS};

use itertools::Itertools;
use log::info;
use rusqlite::{params, Connection, OpenFlags};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const MEMORY_CONN_URI: &str = "file::memory:?cache=shared";

pub struct DbReader {
    conn: Connection,
}

impl DbReader {
    pub fn new() -> ResultT<Self> {
        let conn = Connection::open_with_flags(
            MEMORY_CONN_URI,
            OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;

        conn.execute("PRAGMA read_uncommitted = TRUE;", [])?;

        Ok(DbReader { conn })
    }

    pub fn retrieve_benchmarks_waiting(&mut self, limit: Option<u32>) -> ResultT<Vec<Benchmark>> {
        if let Some(limit) = limit {
            let running = self.retrieve_benchcount_of_status(Status::Running, limit)?;
            let processing = self.retrieve_benchcount_of_status(Status::Processing, limit)?;
            self.retrieve_bench_of_status(Status::Waiting, limit - running - processing)
        } else {
            self.retrieve_all_bench_of_status(Status::Waiting)
        }
    }

    fn retrieve_benchcount_of_status(&mut self, status: Status, limit: u32) -> ResultT<u32> {
        let mut stmt_select_benchstatus = self
            .conn
            .prepare_cached(
                "SELECT COUNT(1) FROM \"status_benchmarks\" AS s WHERE s.status = ?1 LIMIT ?2",
            )
            .expect("Issue during benchstatus select query preparation");
        let count = stmt_select_benchstatus
            .query_row(params![status as u8, limit], |row| row.get(0))
            .expect("Issue during benchmark status update!");

        Ok(count)
    }

    fn retrieve_all_bench_of_status(&mut self, status: Status) -> ResultT<Vec<Benchmark>> {
        let mut stmt_select_benchstatus = self
            .conn
            .prepare_cached(
                "SELECT s.bench_id, b.path, b.prefix
                FROM \"status_benchmarks\" AS s
                JOIN \"benchmarks\" AS b ON b.id = s.bench_id
                WHERE s.status = ?1",
            )
            .expect("Issue during benchstatus select query preparation");
        let rows = stmt_select_benchstatus
            .query_map(params![status as u8], |row| {
                let path: String = row.get(1)?;
                let prefix: String = row.get(2)?;
                Ok(Benchmark {
                    id: row.get(0)?,
                    path: PathBuf::from(path),
                    prefix: match prefix.as_str() {
                        "" => None,
                        _ => Some(PathBuf::from(prefix)),
                    },
                })
            })
            .expect("Issue during benchmark status update!");

        let mut res = vec![];
        for row in rows {
            res.push(row.unwrap());
        }
        Ok(res)
    }

    fn retrieve_bench_of_status(&mut self, status: Status, limit: u32) -> ResultT<Vec<Benchmark>> {
        let mut stmt_select_benchstatus = self
            .conn
            .prepare_cached(
                "SELECT s.bench_id, b.path, b.prefix
                FROM \"status_benchmarks\" AS s
                JOIN \"benchmarks\" AS b ON b.id = s.bench_id
                WHERE s.status = ?1
                LIMIT ?2",
            )
            .expect("Issue during benchstatus select query preparation");
        let rows = stmt_select_benchstatus
            .query_map(params![status as u8, limit], |row| {
                let path: String = row.get(1)?;
                let prefix: String = row.get(2)?;
                Ok(Benchmark {
                    id: row.get(0)?,
                    path: PathBuf::from(path),
                    prefix: match prefix.as_str() {
                        "" => None,
                        _ => Some(PathBuf::from(prefix)),
                    },
                })
            })
            .expect("Issue during benchmark status update!");

        let mut res = Vec::with_capacity(limit as usize);
        for row in rows {
            res.push(row.unwrap());
        }
        Ok(res)
    }

    pub fn waiting_count(&mut self) -> ResultT<u64> {
        let mut stmt_count_benchstatus = self
            .conn
            .prepare_cached(
                "SELECT COUNT(1)
                FROM \"status_benchmarks\" AS s
                WHERE s.status == ?1",
            )
            .expect("Issue during benchstatus count query preparation");

        let row_count_done = {
            let mut res = stmt_count_benchstatus.query(params![Status::Waiting as u8])?;
            let row = res.next()?.unwrap();
            let row_count: u64 = row.get(0)?;
            row_count
        };

        Ok(row_count_done)
    }

    pub fn done_count(&mut self) -> ResultT<u64> {
        let mut stmt_count_benchstatus = self
            .conn
            .prepare_cached(
                "SELECT COUNT(1)
                FROM \"status_benchmarks\" AS s
                WHERE s.status == ?1",
            )
            .expect("Issue during benchstatus count query preparation");

        let row_count_done = {
            let mut res = stmt_count_benchstatus.query(params![Status::Done as u8])?;
            let row = res.next()?.unwrap();
            let row_count: u64 = row.get(0)?;
            row_count
        };

        Ok(row_count_done)
    }

    pub fn total_count(&mut self) -> ResultT<u64> {
        let mut stmt_count_benchstatus = self
            .conn
            .prepare_cached("SELECT COUNT(1) FROM \"status_benchmarks\" AS s")
            .expect("Issue during benchstatus count query preparation");

        let row_count_total = {
            let mut res = stmt_count_benchstatus.query(params![])?;
            let row = res.next()?.unwrap();
            let row_count: u64 = row.get(0)?;
            row_count
        };

        Ok(row_count_total)
    }
}

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
            info!("Populating status table...");
            init::populate_status(conn.transaction()?).expect("Issue during status population");
        }

        Ok(DbWriter { conn })
    }

    pub fn write_to_disk(&self) -> ResultT<()> {
        let query = format!("VACUUM INTO '{}'", ARGS.result_db.display());
        self.conn.execute(&query, params![])?;
        Ok(())
    }

    pub fn update_benchmark_status(&mut self, bench_id: u64, status: Status) -> ResultT<()> {
        let mut stmt_update_benchstatus = self
            .conn
            .prepare_cached(
                "INSERT INTO \"status_benchmarks\" (bench_id, status) 
                VALUES (?1, ?2) 
                ON CONFLICT(bench_id) DO UPDATE SET status = ?2",
            )
            .expect("Issue during status benchmark udpate query preparation");
        stmt_update_benchstatus
            .execute(params![bench_id, status as u8])
            .expect("Issue during benchmark status update!");
        Ok(())
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

        let mut srcid_file_map: HashMap<Arc<String>, u64>;
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
                srcid_file_map.insert(Arc::from(file), id);
            }
        }

        // 2. Track usage data of all (used) functions
        if TRACK_FUNCS.clone() {
            for (file, (funcs, _, _)) in &run_result {
                let sid = srcid_file_map.get(file).unwrap();
                for chunk in &funcs
                    .values()
                    .filter(|f| f.usage.load(Ordering::SeqCst) > 0)
                    .chunks(INSERT_BATCH_SIZE)
                {
                    let mut batch_query = String::new();
                    for func in chunk {
                        // TODO: Investigate why functions are replicated across sources
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
                        ON CONFLICT (source_id, name) DO UPDATE 
                        SET benchmark_usage_count = benchmark_usage_count + excluded.benchmark_usage_count;",
                            sid.to_string(),
                            func.name.to_string(),
                            func.start.line.to_string(),
                            func.start.col.to_string(),
                            func.end.line.to_string(),
                            func.end.col.to_string(),
                            func.usage.load(Ordering::SeqCst)
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
                    .filter(|l| l.usage.load(Ordering::SeqCst) > 0)
                    .chunks(INSERT_BATCH_SIZE)
                {
                    let mut batch_query = String::new();
                    for line in chunk {
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
                            line.usage.load(Ordering::SeqCst)
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
