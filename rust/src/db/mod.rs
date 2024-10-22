mod init;
mod stmts;
use crate::runner::GcovRes;
use crate::types::{Benchmark, BenchmarkRun, Status};
use crate::{ResultT, ARGS};

use log::info;
use rusqlite::{params, params_from_iter, Connection};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub struct Db<'a> {
    conn: Rc<Connection>,

    stmts: stmts::Stmts<'a>,
}

impl<'a> Db<'a> {
    pub fn new() -> ResultT<Self> {
        let conn = Rc::new(Connection::open(&ARGS.result_db)?);
        info!("Creating tables...");
        init::create_tables(&conn).expect("Issue during table creation");
        let stmts = stmts::Stmts::new(Rc::clone(&conn))?;

        Ok(Db { conn, stmts })
    }

    pub fn connect() -> ResultT<Self> {
        let conn = Rc::new(Connection::open(&ARGS.result_db)?);
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
        self.retrieve_bench_of_status(Status::WaitingProcessing, limit)
    }

    pub fn retrieve_benchmarks_waiting_for_cvc5(
        &mut self,
        limit: usize,
    ) -> ResultT<Vec<Benchmark>> {
        self.retrieve_bench_of_status(Status::Waiting, limit)
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
                    prefix: PathBuf::from(prefix),
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
        let mut res = stmt.query(params![Status::Waiting as u8])?;
        let row = res.next()?.unwrap();
        let row_count: usize = row.get(0)?;

        Ok(row_count)
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

    pub fn add_gcov_measurement(&self, bench_id: u64, run_result: GcovRes) -> ResultT<()> {
        // TODO: Instead of hardcoding this chunk size detect it at runtime (PRAGMA compile_options)
        let max_var_count = 250000;

        // 1. Ensure all sources exist in DB & retrieve their ids
        let mut src_args = vec![];
        for (file, _) in &run_result {
            src_args.push(file);
        }
        for src_args_chunk in src_args.chunks(max_var_count) {
            let src_placeholders = src_args_chunk
                .iter()
                .map(|_| "(?)")
                .collect::<Vec<_>>()
                .join(", ");
            let src_query = format!(
                "INSERT INTO \"sources\" ( path ) VALUES {}",
                src_placeholders
            );
            self.conn
                .execute(&src_query, params_from_iter(src_args_chunk.iter()))
                .expect("Could not insert src chunk");
        }

        let mut stmt = self.conn.prepare("SELECT id, path FROM \"sources\"")?;
        let rows = stmt.query_map(params![], |row| {
            let id: u64 = row.get(0)?;
            let file: String = row.get(1)?;
            Ok((file, id))
        })?;
        let mut srcid_file_map: HashMap<String, u64> = HashMap::with_capacity(rows.size_hint().0);
        for row in rows {
            let (file, id) = row?;
            srcid_file_map.insert(file, id);
        }

        // 2. Ensure all functions exist in DB & retrieve their ids
        let mut func_args: Vec<String> = vec![];
        for (file, (funcs, _)) in &run_result {
            let sid = srcid_file_map.get(file).unwrap();
            for func in funcs {
                let f_args: String = format!(
                    "({}, {}, {}, {}, {}, {})",
                    sid.to_string(),
                    func.name.to_string(),
                    func.start.line.to_string(),
                    func.start.col.to_string(),
                    func.end.line.to_string(),
                    func.end.col.to_string(),
                );
                func_args.push(f_args);
            }
        }

        for func_args_chunk in func_args.chunks(max_var_count) {
            let func_placeholders = func_args_chunk
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");

            let func_query = format!(
                "INSERT INTO \"functions\" (
                source_id,
                name,
                start_line,
                start_col,
                end_line,
                end_col
            ) VALUES {} ON CONFLICT DO NOTHING",
                func_placeholders
            );
            self.conn
                .execute(&func_query, params_from_iter(func_args_chunk))
                .expect("Could not insert function chunk");
        }

        let mut stmt = self
            .conn
            .prepare("SELECT id, source_id, name FROM \"functions\"")?;
        let rows = stmt.query_map(params![], |row| {
            let id: u64 = row.get(0)?;
            let source_id: u64 = row.get(1)?;
            let name: String = row.get(2)?;
            Ok(((source_id, name), id))
        })?;
        let mut id_fname_map: HashMap<(u64, String), u64> =
            HashMap::with_capacity(rows.size_hint().0);
        for row in rows {
            let (key, value) = row?;
            id_fname_map.insert(key, value);
        }

        // 3. Insert all function usage data into the DB
        let mut funcusage_args: Vec<u64> = vec![];
        for (file, (funcs, _)) in &run_result {
            let sid = srcid_file_map.get(file).unwrap();
            for func in funcs {
                let funcid = id_fname_map.get(&(*sid, func.name.to_string())).unwrap();
                let mut f_args: Vec<u64> = vec![bench_id, *funcid, func.usage.into()];
                funcusage_args.append(&mut f_args);
            }
        }

        for funcusage_args_chunk in func_args.chunks(max_var_count / 3) {
            let funcusage_placeholders = funcusage_args_chunk
                .iter()
                .map(|_| "(?, ?, ?)")
                .collect::<Vec<_>>()
                .join(", ");

            let funcusage_query = format!(
                "INSERT INTO \"usage_functions\" (
                bench_id,
                func_id,
                usage,
            ) VALUES {}",
                funcusage_placeholders
            );
            self.conn
                .execute(&funcusage_query, params_from_iter(funcusage_args_chunk))
                .expect("Could not insert function usage chunk");
        }

        // 4. Ensure all lines exist in DB & retrieve their ids
        let mut line_args: Vec<u64> = vec![];
        for (file, (_, lines)) in &run_result {
            let sid = srcid_file_map.get(file).unwrap();
            for line in lines {
                let mut l_args: Vec<u64> = vec![*sid, line.line_no.into()];
                line_args.append(&mut l_args);
            }
        }

        for line_args_chunk in line_args.chunks(max_var_count / 2) {
            let line_placeholders = line_args_chunk
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");

            let func_query = format!(
                "INSERT INTO \"lines\" (
                source_id,
                line_no
            ) VALUES {} ON CONFLICT DO NOTHING",
                line_placeholders
            );
            self.conn
                .execute(&func_query, params_from_iter(line_args_chunk))
                .expect("Could not insert line chunk");
        }

        let mut stmt = self
            .conn
            .prepare("SELECT id, source_id, line_no FROM \"lines\"")?;
        let rows = stmt.query_map(params![], |row| {
            let id: u64 = row.get(0)?;
            let source_id: u64 = row.get(1)?;
            let line_no: u64 = row.get(2)?;
            Ok(((source_id, line_no), id))
        })?;
        let mut id_line_map: HashMap<(u64, u64), u64> = HashMap::with_capacity(rows.size_hint().0);
        for row in rows {
            let (key, value) = row?;
            id_line_map.insert(key, value);
        }
        // 5. Insert all line usage data into the DB
        let mut lineusage_args: Vec<u64> = vec![];
        for (file, (_, lines)) in &run_result {
            let sid = srcid_file_map.get(file).unwrap();
            for line in lines {
                let lineid = id_line_map.get(&(*sid, line.line_no.into())).unwrap();
                let mut f_args: Vec<u64> = vec![bench_id, *lineid, line.usage.into()];
                lineusage_args.append(&mut f_args);
            }
        }

        for lineusage_args_chunk in lineusage_args.chunks(max_var_count / 3) {
            let lineusage_placeholders = lineusage_args_chunk
                .iter()
                .map(|_| "(?, ?, ?)")
                .collect::<Vec<_>>()
                .join(", ");

            let lineusage_query = format!(
                "INSERT INTO \"usage_lines\" (
                bench_id,
                line_id,
                usage,
            ) VALUES {}",
                lineusage_placeholders
            );
            self.conn
                .execute(&lineusage_query, params_from_iter(lineusage_args_chunk))
                .expect("Could not insert line usage chunk");
        }

        Ok(())
    }
}
