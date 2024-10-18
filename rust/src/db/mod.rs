mod init;
mod stmts;
use crate::types::{Benchmark, BenchmarkRun, Status};
use crate::{ResultT, ARGS};

use log::info;
use rusqlite::{params, Connection};
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

    // TODO IMplement me
    pub fn add_gcov_measurement(&self) -> ResultT<()> {
        Ok(())
    }
}
