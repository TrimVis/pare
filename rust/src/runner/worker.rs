use super::cvc5;
use super::gcov;
use super::ProcessingQueueMessage;
use super::ProcessingStatusMessage;
use super::RunnerQueueMessage;
use crate::db::DbWriter;
use crate::runner::gcov::merge_gcov;
use crate::runner::GcovRes;
use crate::ARGS;

use crossbeam::channel;
use log::debug;
use log::{error, info, warn};
use std::fs::create_dir_all;
use std::mem;
use std::path::Path;
use std::process::exit;
use std::thread;
use std::time::Instant;

// Worker struct (represents a worker thread)
pub(super) struct Worker {
    _id: usize,
    _thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub(super) fn new_cmd(
        id: usize,
        receiver: channel::Receiver<RunnerQueueMessage>,
        processing_queue: channel::Sender<ProcessingQueueMessage>,
    ) -> Worker {
        let thread = thread::spawn(move || {
            loop {
                let cvc5cmd = Path::join(&ARGS.build_dir, "bin/cvc5");

                let job = receiver.recv();
                match job {
                    Ok(RunnerQueueMessage::Start(benchmark)) => {
                        info!("[Worker {}] Received job (bench_id: {})", id, benchmark.id);
                        let start = Instant::now();
                        let cvc5_result = cvc5::process(&cvc5cmd, &benchmark).unwrap();
                        let res_exit = cvc5_result.exit_code;
                        debug!(
                            "[Worker {}] Ran cvc5 in {}ms (bench_id: {})",
                            id,
                            start.elapsed().as_millis(),
                            benchmark.id
                        );

                        let bench_id = benchmark.id;
                        // Coverage reports are not a thing if the process didn't terminate gracefully
                        if res_exit == 0 {
                            let start = Instant::now();
                            let gcov_result = gcov::process(&benchmark);
                            debug!(
                                "[Worker {}] Processed & ran gcov in {}ms (bench_id: {})",
                                id,
                                start.elapsed().as_millis(),
                                benchmark.id
                            );
                            match processing_queue.send(ProcessingQueueMessage::Result(
                                benchmark.id,
                                cvc5_result,
                                Some(gcov_result),
                            )) {
                                Ok(_) => {
                                    debug!(
                                        "[Worker {}] Queued GCOV result (bench_id: {})",
                                        id, bench_id
                                    );
                                }
                                Err(_) => {
                                    warn!("Worker could not send gcov result to DB writer");
                                    break;
                                }
                            }
                        } else {
                            match processing_queue.send(ProcessingQueueMessage::Result(
                                benchmark.id,
                                cvc5_result,
                                None,
                            )) {
                                Ok(_) => {
                                    debug!(
                                        "[Worker {}] Queued GCOV result (bench_id: {})",
                                        id, bench_id
                                    );
                                }
                                Err(_) => {
                                    warn!("Worker could not send gcov result to DB writer");
                                    break;
                                }
                            }
                            debug!(
                                "[Worker {}] Cvc5 Exit Code was {}... Skipping gcov",
                                id, res_exit
                            );
                        }
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
        status_sender: channel::Sender<ProcessingStatusMessage>,
        receiver: channel::Receiver<ProcessingQueueMessage>,
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
                Ok(_) => status_sender
                    .send(ProcessingStatusMessage::DbInitSuccess)
                    .unwrap(),
                Err(_) => {
                    status_sender
                        .send(ProcessingStatusMessage::DbInitError)
                        .unwrap();
                    error!("Error during DB initialization... terminating DB writer early");
                    exit(1);
                }
            };
            let mut db = db.unwrap();
            status_sender
                .send(ProcessingStatusMessage::Benchmarks(
                    db.get_all_benchmarks()
                        .expect("Could not retrieve benchmarks"),
                ))
                .unwrap();

            // Batch process 100 results at once to decrease load on DB
            const RESULT_BUF_CAPACITY: usize = 100;
            let mut result_buf: Vec<GcovRes> = Vec::with_capacity(RESULT_BUF_CAPACITY);

            loop {
                let job = receiver.recv();
                match job {
                    Ok(ProcessingQueueMessage::Result(bench_id, cvc5_result, gcov_result)) => {
                        let start = Instant::now();
                        info!("[DB Writer] Received a result (bench_id: {})", bench_id);
                        debug!(
                            "[DB Writer] Writing cvc5 result to DB (bench_id: {})",
                            bench_id
                        );
                        db.add_cvc5_run_result(cvc5_result).unwrap();
                        if let Some(gcov_result) = gcov_result {
                            debug!(
                            "[DB Writer] Enqueing GCOV result for later processing (bench_id: {})",
                            bench_id
                        );
                            result_buf.push(gcov_result);
                        }
                        status_sender
                            .send(ProcessingStatusMessage::BenchDone(bench_id))
                            .expect("Could not update bench status");
                        debug!(
                            "[DB Writer] Processed result in {}ms (bench_id: {})",
                            start.elapsed().as_millis(),
                            bench_id
                        );
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

                if result_buf.len() == RESULT_BUF_CAPACITY {
                    info!(
                        "[DB Writer] Merging buffered gcov results & writing to DB. (buf_size: {})",
                        RESULT_BUF_CAPACITY
                    );
                    let start = Instant::now();
                    let result = merge_gcov(result_buf);
                    debug!(
                        "[DB Writer] Merged GCOV results in {}ms",
                        start.elapsed().as_millis()
                    );
                    let start = Instant::now();
                    db.add_gcov_measurement(result)
                        .expect("Could not add gcov measurement");
                    result_buf = Vec::with_capacity(RESULT_BUF_CAPACITY);
                    debug!(
                        "[DB Writer] Inserted merged GCOV result in {}ms",
                        start.elapsed().as_millis()
                    );
                }
            }

            db.write_to_disk()
                .expect("Issue while writing result db to disk");
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
