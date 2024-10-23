use super::cvc5;
use super::gcov;
use super::ProcessingQueueMessage;
use super::RunnerQueueMessage;
use crate::db::Db;
use crate::types::Status;
use crate::ARGS;

use log::{error, info, warn};
use std::mem;
use std::path::Path;
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
        processing_queue: Arc<Mutex<mpsc::Sender<ProcessingQueueMessage>>>,
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
                            .lock()
                            .unwrap()
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
                            .lock()
                            .unwrap()
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

    pub(super) fn new_processing(receiver: mpsc::Receiver<ProcessingQueueMessage>) -> Worker {
        let thread = thread::spawn(move || {
            let mut db = Db::connect().expect("Could not connect to the DB in worker");
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
