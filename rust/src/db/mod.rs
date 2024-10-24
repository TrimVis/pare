mod init;
mod stmts;
use crate::args::{CoverageMode, DB_USAGE_NAME, TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::runner::GcovRes;
use crate::types::{Benchmark, BenchmarkRun, Status};
use crate::{ResultT, ARGS};

use log::info;
use rusqlite::{params, Connection};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub struct Db<'a> {
    conn: Rc<RefCell<Connection>>,

    stmts: stmts::Stmts<'a>,
}

impl<'a> Db<'a> {
    pub fn new() -> ResultT<Self> {
        let conn = Rc::new(RefCell::new(Connection::open(&ARGS.result_db)?));
        info!("Creating tables...");
        init::create_tables(&conn).expect("Issue during table creation");
        let stmts = stmts::Stmts::new(Rc::clone(&conn))?;

        Ok(Db { conn, stmts })
    }

    pub fn connect() -> ResultT<Self> {
        let conn = Rc::new(RefCell::new(Connection::open(&ARGS.result_db)?));
        let stmts = stmts::Stmts::new(Rc::clone(&conn))?;

        Ok(Db { conn, stmts })
    }

    pub fn init(&self) -> ResultT<()> {
        info!("Configuring database...");
        init::prepare(&self.conn).expect("Issue during table preparation");
        info!("Populating config table...");
        init::populate_config(&self.conn).expect("Issue during config table population");
        info!("Populating benchmarks table...");
        init::populate_benchmarks(&self.conn).expect("Issue during benchmark table population");
        info!("Populating status table...");
        init::populate_status(&self.conn).expect("Issue during status population");

        Ok(())
    }

    pub fn update_benchmark_status(&mut self, bench_id: u64, status: Status) -> ResultT<()> {
        self.stmts
            .update_benchstatus
            .borrow_mut()
            .execute(params![bench_id, status as u8])
            .expect("Issue during benchmark status update!");
        Ok(())
    }

    pub fn retrieve_benchmarks_waiting_for_processing(
        &mut self,
        limit: usize,
    ) -> ResultT<Vec<Benchmark>> {
        let enqueued = self
            .retrieve_bench_of_status(Status::Processing, limit)?
            .len();
        self.retrieve_bench_of_status(Status::WaitingProcessing, limit - enqueued)
    }

    pub fn retrieve_benchmarks_waiting_for_cvc5(
        &mut self,
        limit: usize,
    ) -> ResultT<Vec<Benchmark>> {
        let enqueued = self
            .retrieve_bench_of_status(Status::Processing, limit)?
            .len();
        self.retrieve_bench_of_status(Status::Waiting, limit - enqueued)
    }

    fn retrieve_bench_of_status(
        &mut self,
        status: Status,
        limit: usize,
    ) -> ResultT<Vec<Benchmark>> {
        let mut stmt = self.stmts.select_benchstatus.borrow_mut();
        let rows = stmt
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

        let mut res = Vec::with_capacity(limit);
        for row in rows {
            res.push(row.unwrap());
        }
        Ok(res)
    }

    pub fn remaining_count(&mut self) -> ResultT<usize> {
        let mut stmt = self.stmts.count_benchstatus.borrow_mut();
        let row_count_done = {
            let mut res = stmt.query(params![Status::Done as u8])?;
            let row = res.next()?.unwrap();
            let row_count: usize = row.get(0)?;
            row_count
        };
        let mut stmt = self.stmts.count_benchstatus_total.borrow_mut();
        let row_count_total = {
            let mut res = stmt.query(params![])?;
            let row = res.next()?.unwrap();
            let row_count: usize = row.get(0)?;
            row_count
        };

        Ok(row_count_total - row_count_done)
    }

    pub fn add_cvc5_run_result(&mut self, run_result: BenchmarkRun) -> ResultT<()> {
        self.stmts
            .insert_cvc5result
            .borrow_mut()
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

    pub fn add_gcov_measurement(&mut self, bench_id: u64, run_result: GcovRes) -> ResultT<()> {
        // 1. Ensure all sources exist in DB & retrieve their ids
        {
            let mut conn = self.conn.borrow_mut();
            let src_tx = conn.transaction()?;
            {
                let mut src_stmt =
                    src_tx.prepare("INSERT INTO \"sources\" ( path ) VALUES ( ?1 )")?;
                for (file, _) in &run_result {
                    src_stmt.execute(params![file])?;
                }
            }
            src_tx.commit()?;
        }

        let mut srcid_file_map: HashMap<String, u64>;
        {
            let conn = self.conn.borrow_mut();
            let mut stmt = conn.prepare("SELECT id, path FROM \"sources\"")?;
            let rows = stmt.query_map(params![], |row| {
                let id: u64 = row.get(0)?;
                let file: String = row.get(1)?;
                Ok((file, id))
            })?;
            srcid_file_map = HashMap::with_capacity(rows.size_hint().0);
            for row in rows {
                let (file, id) = row?;
                srcid_file_map.insert(file, id);
            }
        }

        // 2. Ensure all (used) functions exist in DB & retrieve their ids
        if TRACK_FUNCS.clone() {
            {
                let mut conn = self.conn.borrow_mut();
                let func_tx = conn.transaction()?;
                {
                    let mut func_stmt = func_tx.prepare(
                        "INSERT INTO \"functions\" (
                            source_id,
                            name,
                            start_line,
                            start_col,
                            end_line,
                            end_col
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )?;
                    for (file, (funcs, _, _)) in &run_result {
                        let sid = srcid_file_map.get(file).unwrap();
                        for func in funcs {
                            if func.usage == 0 {
                                continue;
                            }
                            func_stmt.execute(params![
                                sid.to_string(),
                                func.name.to_string(),
                                func.start.line.to_string(),
                                func.start.col.to_string(),
                                func.end.line.to_string(),
                                func.end.col.to_string(),
                            ])?;
                        }
                    }
                }
                func_tx.commit()?;
            }

            let mut id_fname_map: HashMap<(u64, String), u64>;
            {
                let conn = self.conn.borrow_mut();
                let mut stmt = conn.prepare("SELECT id, source_id, name FROM \"functions\"")?;
                let rows = stmt.query_map(params![], |row| {
                    let id: u64 = row.get(0)?;
                    let source_id: u64 = row.get(1)?;
                    let name: String = row.get(2)?;
                    Ok(((source_id, name), id))
                })?;
                id_fname_map = HashMap::with_capacity(rows.size_hint().0);
                for row in rows {
                    let (key, value) = row?;
                    id_fname_map.insert(key, value);
                }
            }

            // 3. Insert all function usage data into the DB
            {
                let mut conn = self.conn.borrow_mut();
                let funcusage_tx = conn.transaction()?;
                {
                    let funcusage_query = if ARGS.mode == CoverageMode::Full {
                        format!(
                            "INSERT INTO \"usage_functions\" (
                                bench_id,
                                func_id,
                                {0}
                            ) VALUES (?1, ?2, ?3)",
                            DB_USAGE_NAME.clone()
                        )
                    } else {
                        format!(
                            "INSERT INTO \"usage_functions\" (
                                func_id,
                                {0}
                            ) VALUES (?2, ?3)",
                            DB_USAGE_NAME.clone()
                        )
                    };
                    let mut funcusage_stmt = funcusage_tx.prepare(&funcusage_query)?;
                    for (file, (funcs, _, _)) in &run_result {
                        let sid = srcid_file_map.get(file).unwrap();
                        for func in funcs {
                            if func.usage == 0 {
                                continue;
                            }
                            let funcid = id_fname_map.get(&(*sid, func.name.to_string())).unwrap();

                            funcusage_stmt.execute(params![bench_id, *funcid, func.usage])?;
                        }
                    }
                }

                funcusage_tx.commit()?;
            }
        }

        // 4. Ensure all lines exist in DB & retrieve their ids
        if TRACK_LINES.clone() {
            {
                let mut conn = self.conn.borrow_mut();
                let line_tx = conn.transaction()?;
                {
                    let mut line_stmt = line_tx.prepare(
                        "INSERT INTO \"lines\" (
                            source_id,
                            line_no
                        ) VALUES (?1, ?2)",
                    )?;
                    for (file, (_, lines, _)) in &run_result {
                        let sid = srcid_file_map.get(file).unwrap();
                        for line in lines {
                            if line.usage == 0 {
                                continue;
                            }
                            line_stmt.execute(params![*sid, line.line_no])?;
                        }
                    }
                }
                line_tx.commit()?;
            }

            let mut id_line_map: HashMap<(u64, u64), u64>;
            {
                let conn = self.conn.borrow();
                let mut stmt = conn.prepare("SELECT id, source_id, line_no FROM \"lines\"")?;
                let rows = stmt.query_map(params![], |row| {
                    let id: u64 = row.get(0)?;
                    let source_id: u64 = row.get(1)?;
                    let line_no: u64 = row.get(2)?;
                    Ok(((source_id, line_no), id))
                })?;
                id_line_map = HashMap::with_capacity(rows.size_hint().0);
                for row in rows {
                    let (key, value) = row?;
                    id_line_map.insert(key, value);
                }
            }
            // 5. Insert all line usage data into the DB
            {
                let mut conn = self.conn.borrow_mut();
                let lineusage_tx = conn.transaction()?;
                {
                    let lineusage_query = if ARGS.mode == CoverageMode::Full {
                        format!(
                            "INSERT INTO \"usage_lines\" (
                            bench_id,
                            line_id,
                            {0}
                        ) VALUES (?1, ?2, ?3)",
                            DB_USAGE_NAME.clone()
                        )
                    } else {
                        format!(
                            "INSERT INTO \"usage_lines\" (
                            line_id,
                            {0}
                        ) VALUES (?2, ?3)",
                            DB_USAGE_NAME.clone()
                        )
                    };
                    let mut lineusage_stmt = lineusage_tx.prepare(&lineusage_query)?;
                    for (file, (_, lines, _)) in &run_result {
                        let sid = srcid_file_map.get(file).unwrap();
                        for line in lines {
                            if line.usage == 0 {
                                continue;
                            }
                            let lineid = id_line_map.get(&(*sid, line.line_no.into())).unwrap();
                            lineusage_stmt.execute(params![bench_id, *lineid, line.usage])?;
                        }
                    }
                }
                lineusage_tx.commit()?;
            }
        }

        if TRACK_BRANCHES.clone() {
            // TODO: Add support for branch tracking
            unimplemented!("Branch tracking not yet supported")
        }

        Ok(())
    }
}
