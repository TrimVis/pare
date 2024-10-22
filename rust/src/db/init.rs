use crate::types::Status;
use crate::{ResultT, ARGS};

use glob::glob;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs;
use std::path::Path;

pub(super) fn prepare(conn: &RefCell<Connection>) -> ResultT<()> {
    let conn = conn.borrow_mut();
    // Disable disk sync after every transaction
    conn.execute("PRAGMA synchronous = OFF", [])?;
    // Increase cache size to 1GB
    conn.execute("PRAGMA cache_size = -1000000", [])?;
    // Increase page size to 64kB
    conn.execute("PRAGMA page_size = 65536", [])?;
    // Store temporary tables in memory
    conn.execute("PRAGMA temp_store = MEMORY", [])?;
    // // Enable support for foreign keys
    // conn.execute("PRAGMA foreign_keys = ON", [])?;
    // Enable write-ahead-log for increased write performance
    conn.query_row("PRAGMA journal_mode = WAL", [], |_row| Ok(()))?;

    Ok(())
}

pub(super) fn create_tables(conn: &RefCell<Connection>) -> ResultT<()> {
    let conn = conn.borrow_mut();
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
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                UNIQUE(source_id, name)
            )";
    conn.execute(&func_table, [])
        .expect("Issue during functions table creation");

    // Store information about branches
    let branch_table = "CREATE TABLE IF NOT EXISTS \"branches\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                branch_no INTEGER NOT NULL,
                UNIQUE(source_id, branch_no)
            )";
    conn.execute(&branch_table, [])
        .expect("Issue during branches table creation");

    // Store information about lines
    let line_table = "CREATE TABLE IF NOT EXISTS \"lines\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                line_no INTEGER NOT NULL,
                UNIQUE(source_id, line_no)
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
                stderr TEXT NOT NULL
            )";
    conn.execute(&results_table, [])
        .expect("Issue during result_benchmarks table creation");

    // Stores the function usage
    let func_usage_table = "CREATE TABLE IF NOT EXISTS \"usage_functions\" (
                id INTEGER PRIMARY KEY,
                bench_id INTEGER NOT NULL,
                func_id INTEGER NOT NULL,
                usage INTEGER NOT NULL,
                UNIQUE(func_id, bench_id)
            )";
    conn.execute(&func_usage_table, [])
        .expect("Issue during usage_functions table creation");

    // Stores the line usage
    let line_usage_table = "CREATE TABLE IF NOT EXISTS \"usage_lines\" (
                id INTEGER PRIMARY KEY,
                bench_id INTEGER NOT NULL,
                line_id INTEGER NOT NULL,
                usage INTEGER NOT NULL,
                UNIQUE(line_id, bench_id)
            )";
    conn.execute(&line_usage_table, [])
        .expect("Issue during usage_lines table creation");

    // Stores the branch usage
    let branch_usage_table = "CREATE TABLE IF NOT EXISTS \"usage_branches\" (
                id INTEGER PRIMARY KEY,
                bench_id INTEGER NOT NULL,
                branch_id INTEGER NOT NULL,
                usage INTEGER NOT NULL,
                UNIQUE(branch_id, bench_id)
            )";
    conn.execute(&branch_usage_table, [])
        .expect("Issue during usage_branches table creation");

    Ok(())
}

pub(super) fn populate_config(conn: &RefCell<Connection>) -> ResultT<()> {
    let conn = conn.borrow_mut();
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

pub(super) fn populate_benchmarks(conn: &RefCell<Connection>) -> ResultT<()> {
    let conn = conn.borrow_mut();
    // TODO: Readd sampling support
    let mut stmt = conn.prepare("INSERT INTO \"benchmarks\" (path, prefix) VALUES (?1, ?2)")?;

    let prefix_base = ARGS.tmp_dir.as_ref().unwrap();
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

            // TODO: Instead of storing the full path only store the difference
            // due to file size reasons

            stmt.execute(params![file, prefix])?;
        }
    }

    Ok(())
}

pub(super) fn populate_status(conn: &RefCell<Connection>) -> ResultT<()> {
    let conn = conn.borrow_mut();
    // TODO: Readd sampling support
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
