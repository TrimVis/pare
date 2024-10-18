use glob::glob;
use serde_json::Value;
use std::mem;
use std::os::unix::fs::symlink;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Instant;

use log::{error, info};
use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::types::{Benchmark, BenchmarkRun, Status as BenchStatus};
use crate::ARGS;

type BenchmarkFile = PathBuf;
type GcdaFile = PathBuf;
type GcovPrefix = String;

enum QueueMessage {
    GcovCmd(Benchmark),
    Cvc5Cmd(Benchmark),
    Stop,
}

pub struct Runner {
    _workers: Vec<Worker>,
    _wqueue: mpsc::Sender<QueueMessage>,
}

impl Runner {
    pub fn new() -> Self {
        let no_workers = ARGS.job_size;

        assert!(no_workers > 0);

        let (wsender, wreceiver) = mpsc::channel();
        let wreceiver = Arc::new(Mutex::new(wreceiver));

        let mut workers = Vec::with_capacity(no_workers);
        for id in 0..no_workers {
            workers.push(Worker::new(id, Arc::clone(&wreceiver)));
        }

        Self {
            _workers: workers,
            _wqueue: wsender,
        }
    }

    pub fn enqueue_gcov(&self, db: &mut Db, benchmark: Benchmark) {
        db.update_benchmark_status(benchmark.id, BenchStatus::Processing)
            .expect("Could not update benchmark status");
        self._wqueue.send(QueueMessage::Cvc5Cmd(benchmark)).unwrap();
    }

    pub fn enqueue_cvc5(&self, db: &mut Db, benchmark: Benchmark) {
        db.update_benchmark_status(benchmark.id, BenchStatus::Running)
            .expect("Could not update benchmark status");
        self._wqueue.send(QueueMessage::Cvc5Cmd(benchmark)).unwrap();
    }

    pub fn stop(&mut self) {
        self._wqueue.send(QueueMessage::Stop).unwrap();
    }

    pub fn join(&mut self) {
        for worker in &mut self._workers {
            worker.join()
        }
    }
}

// Worker struct (represents a worker thread)
struct Worker {
    _id: usize,
    _thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn process_cvc5(db: &mut Db, cvc5cmd: &Path, benchmark: &Benchmark) -> () {
        let cmd = &mut Command::new(cvc5cmd);
        let cmd = cmd
            .env("GCOV_PREFIX", benchmark.prefix.display().to_string())
            .args(&[&ARGS.cvc5_args, &benchmark.path.display().to_string()]);

        let start = Instant::now();
        let output = cmd.output().expect("Could not capture output of cvc5...");
        let duration = start.elapsed();

        if !output.status.success() {
            error!(
                "CVC5 failed with error!\n Benchmark File: {:?} \n ERROR: {:?}",
                &benchmark.path, &output.stderr
            );
        }

        db.add_cvc5_run_result(BenchmarkRun {
            bench_id: benchmark.id,
            exit_code: output.status.code().unwrap(),
            time_ms: duration
                .as_millis()
                .try_into()
                .expect("Duration too long for 64 bits"),
            stdout: Some(String::from_utf8(output.stdout).expect("Error decoding cvc5 stdout")),
            stderr: Some(String::from_utf8(output.stderr).expect("Error decoding cvc5 stderr")),
        })
        .unwrap();
        db.update_benchmark_status(benchmark.id, BenchStatus::WaitingProcessing)
            .unwrap();
    }

    fn process_gcov(db: &mut Db, benchmark: &Benchmark) -> () {
        // let mut stmt = conn.prepare("INSERT INTO \"sources\" (path, prefix) VALUES (?1, ?2)")?;

        // let build_dir = ARGS.build_dir;
        // let build_dir = build_dir.canonicalize().unwrap().display().to_string();
        // let pattern = format!("{}/**/*.gcno", build_dir);

        // for entry in glob(&pattern).expect("Failed to read glob pattern") {
        //     if let Ok(file) = entry {
        //         // FIXME: This will be of the form src/CMakeFiles/cvc5-obj.dir/.../*.cpp
        //         // It would be best if I could also strip the CMakeFiles/cvc5-obj.dir
        //         // But first I will have to check it for consistency
        //         let file = file
        //             .strip_prefix(build_dir)
        //             .expect("Error while stripping common prefix from gcno file");
        //         let src_file = file.to_str().unwrap();
        //         let src_file = &src_file[..src_file.len() - 5];
        //         stmt.execute(params![src_file])?;
        //     }
        // }

        // FIXME: Symlink the gcno files here

        let prefix_dir = benchmark.prefix.display().to_string();
        let pattern = format!("{}/**/*.gcda", prefix_dir);

        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            if let Ok(gcda_file) = entry {
                let gcno_file = gcda_file.to_str().unwrap();
                let gcno_file = format!("{}.gcno", &gcno_file[..gcno_file.len() - 5]);
                let gcno_file_src = gcno_file.strip_prefix(&prefix_dir);
                symlink(&gcno_file, &gcno_file).expect("Error while trying to create symlink");

                let args = ["--json", "--stdout", gcda_file.to_str().unwrap()];
                let output = Command::new("gcov")
                    .args(&args)
                    .output()
                    .expect("Could not capture output of gcov...");
                if !output.status.success() {
                    error!(
                        "Gcov failed with error!\n GCDA File: {:?} \n ERROR: {:?}",
                        &gcda_file, &output.stderr
                    );
                    return;
                }

                // FIXME: Postprocessing
                let gcov_json: Value =
                    serde_json::from_slice(&output.stdout).expect("Error parsing gcov json output");
            }
        }

        db.update_benchmark_status(benchmark.id, BenchStatus::Done)
            .unwrap();
    }
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<QueueMessage>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            let mut db = Db::new().expect("Could not connect to the DB in worker");
            let cvc5cmd = Path::join(&ARGS.build_dir, "bin/cvc5");

            let job = receiver.lock().unwrap().recv();
            match job {
                Ok(QueueMessage::Cvc5Cmd(benchmark)) => {
                    info!("Worker {} got a job; executing.", id);
                    Worker::process_cvc5(&mut db, &cvc5cmd, &benchmark)
                }
                Ok(QueueMessage::GcovCmd(benchmark)) => {
                    info!("Worker {} got a job; executing.", id);
                    Worker::process_gcov(&mut db, &benchmark)
                }
                Ok(QueueMessage::Stop) => {
                    info!("Worker {} received stop signal; shutting down.", id);
                    break;
                }
                Err(_) => {
                    error!("Worker {} disconnected; shutting down.", id);
                    break;
                }
            }
        });

        Worker {
            _id: id,
            _thread: Some(thread),
        }
    }

    fn join(&mut self) {
        // Join thread and replace with None
        if let Some(_) = self._thread {
            mem::replace(&mut self._thread, None)
                .unwrap()
                .join()
                .expect("Error during worker process join in runner...");
        }
    }
}
