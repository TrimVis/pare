use std::mem;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Instant;

use log::{error, info, warn};
use std::path::{Path, PathBuf};

use crate::db::{Benchmark, BenchmarkRun, Db, Status as BenchStatus};
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
    pub fn new(no_workers: usize) -> Self {
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

    pub fn enqueue_gcov(&self, db: &Db, benchmark: Benchmark) {
        db.update_benchmark_status(benchmark.id, BenchStatus::Processing);
        self._wqueue.send(QueueMessage::Cvc5Cmd(benchmark)).unwrap();
    }

    pub fn enqueue_cvc5(&self, db: &Db, benchmark: Benchmark) {
        db.update_benchmark_status(benchmark.id, BenchStatus::Running);
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
    fn process_cvc5(db: &Db, cvc5cmd: &Path, benchmark: &Benchmark) -> () {
        let mut cmd = &mut Command::new(cvc5cmd)
            .env("GCOV_PREFIX", benchmark.prefix)
            .args(&[ARGS.cvc5_args, benchmark.path.display().to_string()]);

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
        });
        db.update_benchmark_status(benchmark.id, BenchStatus::WaitingProcessing);
    }
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<QueueMessage>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            let db = Db::new().expect("Could not connect to the DB in worker");
            let cvc5cmd = Path::join(&ARGS.build_dir, "bin/cvc5");

            let job = receiver.lock().unwrap().recv();
            match job {
                Ok(QueueMessage::Cvc5Cmd(benchmark)) => {
                    println!("Worker {} got a job; executing.", id);
                    Worker::process_cvc5(&db, &cvc5cmd, &benchmark)
                }
                Ok(QueueMessage::GcovCmd(benchmark)) => {
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

                    db.update_benchmark_status(benchmark.id, BenchStatus::Processing);
                    println!("Worker {} got a job; executing.", id);
                    let mut cmd = &mut Command::new("gcov").args(&[
                        "--json",
                        "--stdout",
                        gcda_file.to_str().unwrap(),
                    ]);

                    if let Some(prefix) = gcov_prefix {
                        cmd.env("GCOV_PREFIX", prefix);
                    }

                    let output = cmd.output().expect("Could not capture output of gcov...");
                    if !output.status.success() {
                        error!("Gcov failed with error!\n GCDA File: {:?} - GCOV Prefix: {:?} \n ERROR: {:?}",
                            &gcda_file,
                            &gcov_prefix,
                            &output.stderr);
                    }
                    db.update_benchmark_status(benchmark.id, BenchStatus::Done);
                }
                Ok(QueueMessage::Stop) => {
                    println!("Worker {} received stop signal; shutting down.", id);
                    break;
                }
                Err(_) => {
                    println!("Worker {} disconnected; shutting down.", id);
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
