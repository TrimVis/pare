use super::cvc5;
use super::gcov;
use super::ProcessingQueueMessage;
use super::RunnerQueueMessage;
use crate::db::DbWriter;
use crate::types::Status;
use crate::ARGS;

use log::{error, info, warn};
use std::fs::create_dir_all;
use std::mem;
use std::path::Path;
use std::process::exit;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

// Worker struct (represents a worker thread)
pub(super) struct Worker {
    _id: usize,
    _thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub(super) fn new_cmd(
        id: usize,
        receiver: Arc<Mutex<mpsc::Receiver<RunnerQueueMessage>>>,
        processing_queue: mpsc::Sender<ProcessingQueueMessage>,
    ) -> Worker {
        let thread = thread::spawn(move || {
            loop {
                let cvc5cmd = Path::join(&ARGS.build_dir, "bin/cvc5");

                let job = receiver.lock().unwrap().recv();
                match job {
                    Ok(RunnerQueueMessage::Cvc5Cmd(benchmark)) => {
                        info!(
                            "[Worker {}] Received cvc5 job (bench_id: {})",
                            id, benchmark.id
                        );
                        let result = cvc5::process(&cvc5cmd, &benchmark).unwrap();
                        processing_queue
                            .send(ProcessingQueueMessage::Cvc5Res(benchmark, result))
                            .unwrap();
                    }
                    Ok(RunnerQueueMessage::GcovCmd(benchmark)) => {
                        info!(
                            "[Worker {}] Received GCOV job (bench_id: {})",
                            id, benchmark.id
                        );

                        let result = gcov::process(&benchmark);
                        processing_queue
                            .send(ProcessingQueueMessage::GcovRes(benchmark, result))
                            .unwrap();
                    }
                    Ok(RunnerQueueMessage::Stop) => {
                        warn!("[Worker {}] Received stop signal.", id);
                        break;
                    }
                    Err(_) => {
                        error!("[Worker {}] Disconnected; shutting down.", id);
                        break;
                    }
                }
            }
            warn!("[Worker {}] Terminated.", id);
        });

        Worker {
            _id: id,
            _thread: Some(thread),
        }
    }

    pub(super) fn new_processing(
        ready_sender: mpsc::Sender<Result<(), ()>>,
        receiver: mpsc::Receiver<ProcessingQueueMessage>,
    ) -> Worker {
        let thread = thread::spawn(move || {
            // Db Setup
            assert!(!ARGS.result_db.exists(), "DB file already exists.");
            let out_dir = ARGS.result_db.parent().unwrap();
            let out_dir = {
                // Just to make sure we can canonicalize it at all
                if out_dir.is_relative() {
                    Path::new("./").join(out_dir).canonicalize().unwrap()
                } else {
                    out_dir.canonicalize().unwrap()
                }
            };
            create_dir_all(out_dir).unwrap();

            let db = DbWriter::new(true);
            match db {
                Ok(_) => ready_sender.send(Ok(())).unwrap(),
                Err(_) => {
                    ready_sender.send(Err(())).unwrap();
                    error!("Error during DB initialization... terminating DB writer early");
                    exit(1);
                }
            };
            let mut db = db.unwrap();

            loop {
                let job = receiver.recv();
                match job {
                    Ok(ProcessingQueueMessage::Cvc5Start(benchmark_id)) => {
                        db.update_benchmark_status(benchmark_id, Status::Running)
                            .expect("Could not update benchmark status");
                    }
                    Ok(ProcessingQueueMessage::Cvc5Res(benchmark, result)) => {
                        info!(
                            "[DB Writer] Received cvc5 result (bench_id: {})",
                            benchmark.id
                        );
                        db.add_cvc5_run_result(result).unwrap();
                        db.update_benchmark_status(benchmark.id, Status::WaitingProcessing)
                            .unwrap();
                    }
                    Ok(ProcessingQueueMessage::GcovStart(benchmark_id)) => {
                        db.update_benchmark_status(benchmark_id, Status::Processing)
                            .expect("Could not update benchmark status");
                    }
                    Ok(ProcessingQueueMessage::GcovRes(benchmark, result)) => {
                        info!(
                            "[DB Writer] Received GCOV result (bench_id: {})",
                            benchmark.id
                        );
                        db.add_gcov_measurement(benchmark.id, result).unwrap();
                        db.update_benchmark_status(benchmark.id, Status::Done)
                            .expect("Could not update bench status");
                    }
                    Ok(ProcessingQueueMessage::Stop) => {
                        warn!("[DB Writer] received stop signal; shutting down.");
                        break;
                    }
                    Err(_) => {
                        warn!("[DB Writer] Disconnected; shutting down.");
                        break;
                    }
                }
            }
            warn!("[DB Writer] Terminated.");
        });

        Worker {
            _id: 0,
            _thread: Some(thread),
        }
    }

    pub(super) fn join(&mut self) {
        // Join thread and replace with None
        if let Some(_) = self._thread {
            mem::replace(&mut self._thread, None)
                .unwrap()
                .join()
                .expect("Error during worker process join in runner...");
        }
    }
}
