use crate::args::{TRACK_BRANCHES, TRACK_FUNCS, TRACK_LINES};
use crate::{ResultT, ARGS};

use glob::glob;
use rusqlite::{params, Connection, Transaction};
use sha2::{Digest, Sha256};
use std::fs;

pub(super) fn prepare(conn: &Connection) -> ResultT<()> {
    // Disable disk sync after every transaction
    conn.execute("PRAGMA synchronous = OFF", [])?;
    // Increase cache size to 10MB
    conn.execute("PRAGMA cache_size = -10000", [])?;
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

pub(super) fn create_tables(conn: &Connection) -> ResultT<()> {
    // Stores the arguments and other run parameters
    let config_table = "CREATE TABLE IF NOT EXISTS \"config\" (
                key TEXT NOT NULL PRIMARY KEY,
                value TEXT NOT NULL
            )";
    conn.execute(&config_table, [])
        .expect("Issue during config table creation");

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
                path INTEGER NOT NULL UNIQUE
            )";
    conn.execute(&source_table, [])
        .expect("Issue during sources table creation");

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

    if TRACK_FUNCS.clone() {
        // Store information about functions and their usage
        let func_table = "CREATE TABLE IF NOT EXISTS \"functions\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                benchmark_usage_count INTEGER NOT NULL,
                UNIQUE(source_id, name)
            )";
        conn.execute(&func_table, [])
            .expect("Issue during functions table creation");
    }

    if TRACK_LINES.clone() {
        // Store information about lines and their usage
        let line_table = "CREATE TABLE IF NOT EXISTS \"lines\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                line_no INTEGER NOT NULL,
                benchmark_usage_count INTEGER NOT NULL,
                UNIQUE(source_id, line_no)
            )";
        conn.execute(&line_table, [])
            .expect("Issue during lines table creation");
    }

    if TRACK_BRANCHES.clone() {
        // Store information about branches
        let branch_table = "CREATE TABLE IF NOT EXISTS \"branches\" (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                branch_no INTEGER NOT NULL,
                benchmark_usage_count INTEGER NOT NULL,
                UNIQUE(source_id, branch_no)
            )";
        conn.execute(&branch_table, [])
            .expect("Issue during branches table creation");
    }

    Ok(())
}

pub(super) fn populate_config(tx: Transaction) -> ResultT<()> {
    let c_insert = "INSERT INTO \"config\" (key, value) VALUES (?1, ?2)";
    tx.execute(
        &c_insert,
        params!["individual_gcov_prefixes", ARGS.individual_prefixes],
    )?;

    for (i, c) in ARGS.coverage_kinds.iter().enumerate() {
        let k = format!("coverage_kind_{}", i);
        tx.execute(&c_insert, params![k, c.to_string()])?;
    }

    tx.execute(&c_insert, params!["job_size", ARGS.job_size])?;

    tx.execute(&c_insert, params!["cvc5_args", ARGS.cvc5_args])?;

    tx.execute(
        &c_insert,
        params!["benchmark_dir", ARGS.benchmark_dir.display().to_string()],
    )?;

    tx.commit()?;

    Ok(())
}

pub(super) fn populate_benchmarks(tx: Transaction) -> ResultT<()> {
    // TODO: Readd sampling support
    {
        let mut stmt = tx.prepare("INSERT INTO \"benchmarks\" (path, prefix) VALUES (?1, ?2)")?;

        let prefix_base = ARGS.tmp_dir.as_ref().unwrap();
        fs::create_dir_all(&prefix_base)
            .expect("Could not create temporary base folder for prefix files");

        let bench_dir = &ARGS.benchmark_dir;
        let bench_dir = bench_dir.canonicalize().unwrap().display().to_string();
        let pattern = format!("{}/**/*.smt2", bench_dir);

        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            if let Ok(file) = entry {
                let dfile = file.canonicalize().unwrap().display().to_string();
                let prefix = if ARGS.individual_prefixes {
                    let mut hasher = Sha256::new();
                    hasher.update(file.to_string_lossy().as_bytes());
                    let hash = format!("{:x}", hasher.finalize());

                    let prefix = prefix_base.join(hash);
                    if !prefix.exists() {
                        fs::create_dir(&prefix).expect("Could not create prefix dir");
                    }

                    let prefix = prefix.canonicalize().unwrap().display().to_string();
                    prefix
                } else {
                    "".to_string()
                };

                // TODO: Instead of storing the full path only store the difference
                // due to file size reasons

                stmt.execute(params![dfile, prefix])?;
            }
        }
    }

    tx.commit()?;

    Ok(())
}
