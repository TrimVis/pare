use std::path::PathBuf;

pub type ResultT<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
pub struct FilePosition {
    pub line: u32,
    pub col: u32,
}

#[allow(dead_code)]
pub struct Function {
    pub id: Option<usize>,
    pub source_id: Option<usize>,
    pub name: String,
    pub start: FilePosition,
    pub end: FilePosition,
}

#[allow(dead_code)]
pub struct Line {
    pub id: Option<usize>,
    pub source_id: Option<usize>,
    pub line_no: u32,
}

#[allow(dead_code)]
pub struct Source {
    pub id: Option<u64>,
    pub path: PathBuf,
}

#[allow(dead_code)]
pub struct FuncBenchUsage {
    pub id: Option<usize>,
    pub bench_id: Option<usize>,
    pub func_id: Option<usize>,
    pub usage: u32,
}

#[allow(dead_code)]
pub struct LineBenchUsage {
    pub id: Option<usize>,
    pub bench_id: Option<usize>,
    pub line_id: Option<usize>,
    pub usage: u32,
}

#[derive(Debug, Clone)]
pub struct Benchmark {
    pub id: u64,
    pub path: PathBuf,
    pub prefix: Option<PathBuf>,
}

pub struct Cvc5BenchmarkRun {
    pub bench_id: u64,
    pub time_ms: u64,
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

use std::sync::atomic::AtomicU32;

#[derive(Debug)]
pub struct GcovFuncResult {
    pub name: String,
    pub start: FilePosition,
    pub end: FilePosition,
    pub usage: AtomicU32,
}

#[derive(Debug)]
pub struct GcovLineResult {
    pub line_no: u32,
    pub usage: AtomicU32,
}

#[derive(Debug, Clone)]
pub struct GcovBranchResult {}
