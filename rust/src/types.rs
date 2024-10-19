use std::path::PathBuf;

pub type ResultT<T> = Result<T, Box<dyn std::error::Error>>;

pub enum Status {
    Waiting,
    Running,
    WaitingProcessing,
    Processing,
    Done,
}

pub struct FilePosition {
    pub line: u32,
    pub col: u32,
}

pub struct Function {
    pub id: Option<usize>,
    pub source_id: Option<usize>,
    pub name: String,
    pub start: FilePosition,
    pub end: FilePosition,
}

pub struct Line {
    pub id: Option<usize>,
    pub source_id: Option<usize>,
    pub line_no: u32,
}

pub struct Source {
    pub id: Option<u64>,
    pub path: PathBuf,
}

pub struct FuncBenchUsage {
    pub id: Option<usize>,
    pub bench_id: Option<usize>,
    pub func_id: Option<usize>,
    pub usage: u32,
}

pub struct LineBenchUsage {
    pub id: Option<usize>,
    pub bench_id: Option<usize>,
    pub line_id: Option<usize>,
    pub usage: u32,
}

// TODO: Switch ids everywhere to option
pub struct Benchmark {
    pub id: u64,
    pub path: PathBuf,
    pub prefix: PathBuf,
}

pub struct BenchmarkRun {
    pub bench_id: u64,
    pub time_ms: u64,
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

pub struct GcovFuncResult {
    pub name: String,
    pub start: FilePosition,
    pub end: FilePosition,
    pub usage: u32,
}

pub struct GcovLineResult {
    pub line_no: u32,
    pub usage: u32,
}
