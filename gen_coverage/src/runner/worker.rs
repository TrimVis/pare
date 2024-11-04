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
use log::LevelFilter;
use log::{error, info, warn};
use std::borrow::BorrowMut;
use std::cmp::min;
use std::fs::create_dir_all;
use std::fs::remove_dir_all;
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
                        let start = if log::max_level() >= LevelFilter::Debug {
                            Some(Instant::now())
                        } else {
                            None
                        };
                        let cvc5_result = cvc5::process(&cvc5cmd, &benchmark).unwrap();
                        let res_exit = cvc5_result.exit_code;
                        if log::max_level() >= LevelFilter::Debug {
                            debug!(
                                "[Worker {}] Ran cvc5 in {}ms (bench_id: {})",
                                id,
                                start.unwrap().elapsed().as_millis(),
                                benchmark.id
                            );
                        }

                        let bench_id = benchmark.id;
                        // Coverage reports are not a thing if the process didn't terminate gracefully
                        if res_exit == 0 {
                            let start = if log::max_level() >= LevelFilter::Debug {
                                Some(Instant::now())
                            } else {
                                None
                            };
                            let gcov_result = gcov::process(&benchmark);

                            if log::max_level() >= LevelFilter::Debug {
                                debug!(
                                    "[Worker {}] Processed & ran gcov in {}ms (bench_id: {})",
                                    id,
                                    start.unwrap().elapsed().as_millis(),
                                    benchmark.id
                                );
                            }

                            // Remove prefix directory
                            match &benchmark.prefix {
                                None => (),
                                Some(v) => remove_dir_all(v).unwrap_or_else(|e| {
                                    debug!("Could not delete prefix dir: {:?}", e)
                                }),
                            };

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
            let bench_count: u64 = {
                let benchmarks = db
                    .get_all_benchmarks()
                    .expect("Could not retrieve benchmarks");
                let count = benchmarks.len();
                status_sender
                    .send(ProcessingStatusMessage::Benchmarks(benchmarks))
                    .unwrap();
                count as u64
            };

            // Batch process 100 results at once to decrease load on DB
            let max_bench_aggregate: u64 = min(100, bench_count);
            let mut result_buf: Option<GcovRes> = None;
            let mut bench_counter: u64 = 0;
            let mut rem_counter: u64 = bench_count;

            loop {
                if rem_counter == 0 {
                    info!("[DB Writer] Processed all benchmarks, breaking out of message loop");
                    break;
                }
                let job = receiver.recv();
                match job {
                    Ok(ProcessingQueueMessage::Result(bench_id, cvc5_result, gcov_result)) => {
                        let start = if log::max_level() >= LevelFilter::Debug {
                            Some(Instant::now())
                        } else {
                            None
                        };
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
                            match result_buf.borrow_mut() {
                                None => {
                                    result_buf = Some(gcov_result);
                                }
                                Some(r) => {
                                    merge_gcov(r, gcov_result);
                                }
                            }
                        }
                        bench_counter += 1;
                        rem_counter -= 1;
                        if log::max_level() >= LevelFilter::Debug {
                            debug!(
                                "[DB Writer] Processed result in {}ms (bench_id: {})",
                                start.unwrap().elapsed().as_millis(),
                                bench_id
                            );
                        }
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

                if bench_counter >= max_bench_aggregate {
                    // Only wake main thread every 20 benchmarks
                    info!("[DB Writer] Writing merged GCOV results to DB");
                    let start = if log::max_level() >= LevelFilter::Debug {
                        Some(Instant::now())
                    } else {
                        None
                    };
                    match result_buf {
                        Some(r) => db
                            .add_gcov_measurement(r)
                            .expect("Could not add gcov measurement"),
                        None => error!("No results to write out"),
                    };

                    if log::max_level() >= LevelFilter::Debug {
                        debug!(
                            "[DB Writer] Inserted merged GCOV result in {}ms",
                            start.unwrap().elapsed().as_millis()
                        );
                    }

                    status_sender
                        .send(ProcessingStatusMessage::BenchesDone(bench_counter))
                        .expect("Could not update bench status");

                    result_buf = None;
                    bench_counter = 0;
                }
            }

            info!("[DB Writer] Cleaning up.");
            if let Some(r) = result_buf {
                db.add_gcov_measurement(r)
                    .expect("Could not add gcov measurement");
            };
            status_sender
                .send(ProcessingStatusMessage::BenchesDone(bench_counter))
                .expect("Could not update bench status");

            db.write_to_disk()
                .expect("Issue while writing result db to disk");

            info!("[DB Writer] Terminated.");
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
