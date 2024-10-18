use std::path::PathBuf;

pub type ResultT<T> = Result<T, Box<dyn std::error::Error>>;

pub enum Status {
    Waiting,
    Running,
    WaitingProcessing,
    Processing,
    Done,
}

pub struct Source {
    pub id: u64,
    pub path: PathBuf,
}

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
