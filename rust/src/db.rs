use glob::glob;
// use rusqlite::OptionalExtension;
use log::{error, info, warn};
use rusqlite::{params, Connection, Statement};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::types::{Benchmark, BenchmarkRun, Status};
use crate::{ResultT, ARGS};

pub struct Db<'a> {
    conn: Rc<Connection>,

    stmts: Stmts<'a>,
}

impl<'a> Db<'a> {
    pub fn new() -> ResultT<Self> {
        let conn = Rc::new(Connection::open(&ARGS.result_db)?);
        info!("Creating tables...");
        create_tables(&conn).expect("Issue during table creation");
        let stmts = Stmts::new(Rc::clone(&conn))?;

        Ok(Db { conn, stmts })
    }

    pub fn init(&self) -> ResultT<()> {
        info!("Configuring database...");
        prepare(&self.conn).expect("Issue during table preparation");
        info!("Populating config table...");
        populate_config(&self.conn).expect("Issue during config table population");
        info!("Populating benchmarks table...");
        populate_benchmarks(&self.conn).expect("Issue during benchmark table population");
        info!("Populating status table...");
        populate_status(&self.conn).expect("Issue during status population");

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

struct Stmts<'a> {
    insert_cvc5result: Rc<RefCell<Statement<'a>>>,
    update_benchstatus: Rc<RefCell<Statement<'a>>>,
    select_benchstatus: Rc<RefCell<Statement<'a>>>,
    count_benchstatus: Rc<RefCell<Statement<'a>>>,
}
impl<'a> Stmts<'a> {
    pub fn new(conn: Rc<Connection>) -> ResultT<Self> {
        let insert_cvc5result = Rc::new(RefCell::new(
            conn.prepare(
                "INSERT INTO \"result_benchmarks\" (
                bench_id,
                time_ms,
                exit_code,
                stdout,
                stderr
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .expect("Issue during benchmark status update query preparation"),
        ));

        let update_benchstatus = Rc::from(RefCell::new(
            conn.prepare(
                "INSERT INTO \"status_benchmarks\" (bench_id, status) 
                VALUES (?1, ?2) 
                ON CONFLICT(bench_id) DO UPDATE SET status = ?2",
            )
            .expect("Issue during cvc5 run result query preparation"),
        ));

        let select_benchstatus = Rc::from(RefCell::new(
            conn.prepare(
                "SELECT s.bench_id, b.path, b.prefix
                FROM \"status_benchmarks\" AS s
                JOIN \"benchmarks\" AS b ON b.id = s.bench_id
                WHERE s.status = ?1
                LIMIT ?2",
            )
            .expect("Issue during benchstatus select query preparation"),
        ));

        let count_benchstatus = Rc::from(RefCell::new(
            conn.prepare(
                "SELECT COUNT(1)
                FROM \"status_benchmarks\" AS s
                JOIN \"benchmarks\" AS b ON b.id = s.bench_id
                WHERE s.status = ?1",
            )
            .expect("Issue during benchstatus count query preparation"),
        ));

        // FIXME: Dirty hack by ChatGPT. There is likely a better way
        Ok(Stmts {
            insert_cvc5result: unsafe { std::mem::transmute(insert_cvc5result) },
            update_benchstatus: unsafe { std::mem::transmute(update_benchstatus) },
            select_benchstatus: unsafe { std::mem::transmute(select_benchstatus) },
            count_benchstatus: unsafe { std::mem::transmute(count_benchstatus) },
        })
        // Ok(Stmts {
        //     insert_cvc5result,
        //     update_benchstatus,
        //     select_benchstatus,
        // })
    }
}

fn prepare(conn: &Connection) -> ResultT<()> {
    // Disable disk sync after every transaction
    conn.execute("PRAGMA synchronous = OFF", [])?;
    // Increase cache size to 1GB
    conn.execute("PRAGMA cache_size = -1000000", [])?;
    // Store temporary tables in memory
    conn.execute("PRAGMA temp_store = MEMORY", [])?;
    // Enable support for foreign keys
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    // Enable write-ahead-log for increased write performance
    conn.query_row("PRAGMA journal_mode = WAL", [], |_row| Ok(()))?;

    Ok(())
}

fn create_tables(conn: &Connection) -> ResultT<()> {
    // Stores the arguments and other run parameters
    let config_table = "CREATE TABLE IF NOT EXISTS \"config\" (
                key TEXT NOT NULL PRIMARY KEY,
                value TEXT NOT NULL
            )";
    conn.execute(&config_table, [])
        .expect("Issue during config table creation");

    // Stores the processing status for all benchmarks
    let status_table = "CREATE TABLE IF NOT EXISTS \"status_benchmarks\" (
                bench_id INTEGER NOT NULL PRIMARY KEY,
                status TEXT
            )";
    conn.execute(&status_table, [])
        .expect("Issue during benchmark status table creation");

    // Stores the benchmark metadata
    let benchmarks_table = "CREATE TABLE IF NOT EXISTS \"benchmarks\" (
                id INTEGER PRIMARY KEY,
                prefix TEXT,
                path TEXT NOT NULL
            )";
    conn.execute(&benchmarks_table, [])
        .expect("Issue during benchmarks table creation");

    // Store information about source files
    let source_table = "CREATE TABLE IF NOT EXISTS \"sources\" (
                id INTEGER PRIMARY KEY,
                path INTEGER NOT NULL
            )";
    conn.execute(&source_table, [])
        .expect("Issue during sources table creation");

    // Store information about functions
    let func_table = "CREATE TABLE IF NOT EXISTS \"functions\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                UNIQUE(source_id, name),
                FOREIGN KEY (source_id) REFERENCES sources(id)
            )";
    conn.execute(&func_table, [])
        .expect("Issue during functions table creation");

    // Store information about branches
    let branch_table = "CREATE TABLE IF NOT EXISTS \"branches\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                branch_no INTEGER NOT NULL,
                UNIQUE(source_id, branch_no),
                FOREIGN KEY (source_id) REFERENCES sources(id)
            )";
    conn.execute(&branch_table, [])
        .expect("Issue during branches table creation");

    // Store information about lines
    let line_table = "CREATE TABLE IF NOT EXISTS \"lines\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                line_no INTEGER NOT NULL,
                UNIQUE(source_id, line_no),
                FOREIGN KEY (source_id) REFERENCES sources(id)
            )";
    conn.execute(&line_table, [])
        .expect("Issue during lines table creation");

    // Stores the output of benchmark runs and other metadata
    let results_table = "CREATE TABLE IF NOT EXISTS \"result_benchmarks\" (
                id INTEGER PRIMARY KEY,
                bench_id INTEGER NOT NULL,
                time_ms INTEGER NOT NULL,
                exit_code INTEGER NOT NULL,
                stdout TEXT NOT NULL,
                stderr TEXT NOT NULL,
                FOREIGN KEY (bench_id) REFERENCES benchmarks(id)
            )";
    conn.execute(&results_table, [])
        .expect("Issue during result_benchmarks table creation");

    // Stores the function usage
    let func_usage_table = "CREATE TABLE IF NOT EXISTS \"usage_functions\" (
                id INTEGER PRIMARY KEY,
                bench_id INTEGER NOT NULL,
                func_id INTEGER NOT NULL,
                usage TEXT NOT NULL,
                UNIQUE(bench_id, func_id),
                FOREIGN KEY (func_id) REFERENCES functions(id),
                FOREIGN KEY (bench_id) REFERENCES benchmarks(id)
            )";
    conn.execute(&func_usage_table, [])
        .expect("Issue during usage_functions table creation");

    // Stores the line usage
    let line_usage_table = "CREATE TABLE IF NOT EXISTS \"usage_lines\" (
                id INTEGER PRIMARY KEY,
                bench_id INTEGER NOT NULL,
                line_id INTEGER NOT NULL,
                usage TEXT NOT NULL,
                UNIQUE(bench_id, line_id),
                FOREIGN KEY (line_id) REFERENCES lines(id),
                FOREIGN KEY (bench_id) REFERENCES benchmarks(id)
            )";
    conn.execute(&line_usage_table, [])
        .expect("Issue during usage_lines table creation");

    // Stores the branch usage
    let branch_usage_table = "CREATE TABLE IF NOT EXISTS \"usage_branches\" (
                id INTEGER PRIMARY KEY,
                bench_id INTEGER NOT NULL,
                branch_id INTEGER NOT NULL,
                usage TEXT NOT NULL,
                UNIQUE(bench_id, branch_id),
                FOREIGN KEY (branch_id) REFERENCES branches(id),
                FOREIGN KEY (bench_id) REFERENCES benchmarks(id)
            )";
    conn.execute(&branch_usage_table, [])
        .expect("Issue during usage_branches table creation");

    Ok(())
}

fn populate_config(conn: &Connection) -> ResultT<()> {
    let c_insert = "INSERT INTO \"config\" (key, value) VALUES (?1, ?2)";
    conn.execute(
        &c_insert,
        params!["individual_gcov_prefixes", ARGS.individual_prefixes],
    )?;
    conn.execute(&c_insert, params!["sample_size", ARGS.sample_size])?;

    conn.execute(&c_insert, params!["job_size", ARGS.job_size])?;

    conn.execute(&c_insert, params!["cvc5_args", ARGS.cvc5_args])?;

    conn.execute(
        &c_insert,
        params!["benchmark_dir", ARGS.benchmark_dir.display().to_string()],
    )?;

    Ok(())
}

fn populate_benchmarks(conn: &Connection) -> ResultT<()> {
    // FIXME: Readd sampling support
    let mut stmt = conn.prepare("INSERT INTO \"benchmarks\" (path, prefix) VALUES (?1, ?2)")?;

    let prefix_base = Path::new("/tmp/asdf");
    fs::create_dir_all(&prefix_base)
        .expect("Could not create temporary base folder for prefix files");

    let bench_dir = &ARGS.benchmark_dir;
    let bench_dir = bench_dir.canonicalize().unwrap().display().to_string();
    let pattern = format!("{}/**/*.smt2", bench_dir);

    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(file) = entry {
            let mut hasher = Sha256::new();
            hasher.update(file.to_string_lossy().as_bytes());
            let hash = format!("{:x}", hasher.finalize());

            let prefix = prefix_base.join(hash);
            if !prefix.exists() {
                fs::create_dir(&prefix).expect("Could not create prefix dir");
            }

            let file = file.canonicalize().unwrap().display().to_string();
            let prefix = prefix.canonicalize().unwrap().display().to_string();

            // FIXME: Instead of storing the full path only store the difference
            // due to file size reasons
            
            stmt.execute(params![file, prefix])?;
        }
    }

    Ok(())
}

fn populate_status(conn: &Connection) -> ResultT<()> {
    // FIXME: Readd sampling support
    let mut select_stmt = conn.prepare("SELECT id FROM \"benchmarks\"")?;
    let bench_rows = select_stmt.query_map([], |row| {
        let id: u64 = row.get(0)?;
        Ok(id)
    })?;

    let mut stmt =
        conn.prepare("INSERT INTO \"status_benchmarks\" (bench_id, status) VALUES (?1, ?2)")?;
    for row in bench_rows {
        let bench_id = row.unwrap();
        stmt.execute(params![bench_id, Status::Waiting as u64])?;
    }

    Ok(())
}

// NOTE pjordan: This would require us to
fn _populate_sources(conn: &Connection) -> ResultT<()> {
    let mut stmt = conn.prepare("INSERT INTO \"sources\" (path, prefix) VALUES (?1, ?2)")?;

    let build_dir = &ARGS.build_dir;
    let build_dir = build_dir.canonicalize().unwrap().display().to_string();
    let pattern = format!("{}/**/*.gcno", build_dir);

    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        if let Ok(file) = entry {
            // FIXME: This will be of the form src/CMakeFiles/cvc5-obj.dir/.../*.cpp
            // It would be best if I could also strip the CMakeFiles/cvc5-obj.dir
            // But first I will have to check it for consistency
            let file = file
                .strip_prefix(&build_dir)
                .expect("Error while stripping common prefix from gcno file");
            let src_file = file.to_str().unwrap();
            let src_file = &src_file[..src_file.len() - 5];
            stmt.execute(params![src_file])?;
        }
    }

    Ok(())
}
