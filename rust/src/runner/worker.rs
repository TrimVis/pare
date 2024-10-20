use super::cvc5;
use super::gcov;
use super::ProcessingQueueMessage;
use super::RunnerQueueMessage;
use crate::db::Db;
use crate::types::Status;
use crate::ARGS;

use log::{error, info};
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
        let thread = thread::spawn(move || loop {
            let cvc5cmd = Path::join(&ARGS.build_dir, "bin/cvc5");

            let job = receiver.lock().unwrap().recv();
            match job {
                Ok(RunnerQueueMessage::Cvc5Cmd(benchmark)) => {
                    info!("Worker {} got a job; executing.", id);
                    let result = cvc5::process(&cvc5cmd, &benchmark).unwrap();
                    processing_queue
                        .lock()
                        .unwrap()
                        .send(ProcessingQueueMessage::Cvc5Res(benchmark, result))
                        .unwrap();
                }
                Ok(RunnerQueueMessage::GcovCmd(benchmark)) => {
                    info!("Worker {} got a job; executing.", id);

                    let result = gcov::process(&benchmark);
                    processing_queue
                        .lock()
                        .unwrap()
                        .send(ProcessingQueueMessage::GcovRes(benchmark, result))
                        .unwrap();
                }
                Ok(RunnerQueueMessage::Stop) => {
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

    pub(super) fn new_processing(receiver: mpsc::Receiver<ProcessingQueueMessage>) -> Worker {
        let thread = thread::spawn(move || loop {
            let mut db = Db::connect().expect("Could not connect to the DB in worker");

            let job = receiver.recv();
            match job {
                Ok(ProcessingQueueMessage::Cvc5Start(benchmark_id)) => {
                    info!("Processing Worker got a cvc5 start event.");
                    db.update_benchmark_status(benchmark_id, Status::Running)
                        .expect("Could not update benchmark status");
                }
                Ok(ProcessingQueueMessage::Cvc5Res(benchmark, result)) => {
                    info!("Processing Worker got a cvc5 result.");
                    db.add_cvc5_run_result(result).unwrap();
                    db.update_benchmark_status(benchmark.id, Status::WaitingProcessing)
                        .unwrap();
                }
                Ok(ProcessingQueueMessage::GcovStart(benchmark_id)) => {
                    info!("Processing Worker got a gcov start event.");
                    db.update_benchmark_status(benchmark_id, Status::Processing)
                        .expect("Could not update benchmark status");
                }
                Ok(ProcessingQueueMessage::GcovRes(benchmark, result)) => {
                    info!("Processing Worker got a gcov result.");
                    db.add_gcov_measurement(benchmark.id, result).unwrap();
                    db.update_benchmark_status(benchmark.id, Status::Done)
                        .unwrap();
                }
                Ok(ProcessingQueueMessage::Stop) => {
                    info!("Processing Worker received stop signal; shutting down.");
                    break;
                }
                Err(_) => {
                    error!("Processing Worker disconnected; shutting down.");
                    break;
                }
            }
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
