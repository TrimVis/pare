mod gcov;
mod run;
mod worker;
pub use gcov::GcovBitvec;
pub use gcov::GcovRes;
use log::{error, warn};

use crate::types::{Benchmark, BenchmarkRun};
use crate::ARGS;

use crossbeam::channel;
use std::collections::HashSet;
use std::process::exit;

enum RunnerQueueMessage {
    Start(Benchmark),
    Stop,
}

type ProcessingQueueMessage = (u64, BenchmarkRun, Option<GcovRes>);

enum ProcessingStatusMessage {
    DbInitSuccess,
    DbInitError,
    BenchesDone(u64),
    Benchmarks(Vec<Benchmark>),
}

pub struct Runner {
    runner_workers: Vec<worker::Worker>,
    runner_queue: channel::Sender<RunnerQueueMessage>,

    processing_status_queue: channel::Receiver<ProcessingStatusMessage>,
    processing_worker: worker::Worker,

    enqueued: Box<HashSet<u64>>,
}

impl Runner {
    pub fn new() -> Self {
        let no_workers = ARGS.job_size;

        assert!(no_workers > 0);

        let (p_status_send, p_status_receiver) = channel::unbounded();
        let (p_sender, p_receiver) = channel::bounded(10 * ARGS.job_size);
        let processing_queue = p_sender;
        let processing_worker =
            worker::Worker::new_processing(p_status_send.clone(), p_receiver.clone());

        let (r_sender, r_receiver) = channel::unbounded();
        let runner_receiver = r_receiver;
        let runner_queue = r_sender;

        let mut runner_workers = Vec::with_capacity(no_workers);
        for id in 0..no_workers {
            runner_workers.push(worker::Worker::new_cmd(
                id,
                runner_receiver.clone(),
                processing_queue.clone(),
            ));
        }

        Self {
            runner_workers,
            runner_queue,

            processing_worker,
            processing_status_queue: p_status_receiver,

            enqueued: Box::from(HashSet::new()),
        }
    }

    pub fn wait_on_db_ready(&mut self) {
        match self.processing_status_queue.recv().unwrap() {
            ProcessingStatusMessage::DbInitSuccess => {}
            ProcessingStatusMessage::DbInitError => {
                warn!("Could not init DB in DB Writer. Exiting early...");
                exit(1);
            }
            _ => unreachable!("This message was not expected!"),
        }
    }

    pub fn wait_for_all_benchmarks(&mut self) -> Vec<Benchmark> {
        match self.processing_status_queue.recv().unwrap() {
            ProcessingStatusMessage::Benchmarks(res) => res,
            _ => unreachable!("This message was not expected!"),
        }
    }

    pub fn wait_for_next_bench_done(&mut self) -> u64 {
        match self.processing_status_queue.recv().unwrap() {
            ProcessingStatusMessage::BenchesDone(res) => res,
            _ => unreachable!("This message was not expected!"),
        }
    }

    pub fn enqueue(&mut self, benchmark: Benchmark) {
        // Safety guard
        if !self.enqueued.contains(&benchmark.id) {
            self.enqueued.insert(benchmark.id);
            self.runner_queue
                .send(RunnerQueueMessage::Start(benchmark))
                .unwrap();
        }
    }

    pub fn enqueue_worker_stop(&mut self) {
        for _ in &self.runner_workers {
            self.runner_queue
                .send(RunnerQueueMessage::Stop)
                .unwrap_or_else(|e| error!("Could not stop worker: {}", e));
        }
    }

    pub fn join(&mut self) {
        for runner in &mut self.runner_workers {
            runner.join();
        }

        self.processing_worker.join();
    }
}
