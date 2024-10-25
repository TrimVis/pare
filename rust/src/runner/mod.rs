mod cvc5;
mod gcov;
mod worker;
pub use gcov::GcovRes;
use log::warn;

use crate::types::{Benchmark, Cvc5BenchmarkRun};
use crate::ARGS;

use crossbeam::channel;
use std::collections::HashSet;
use std::process::exit;

enum RunnerQueueMessage {
    Start(Benchmark),
    Stop,
}

enum ProcessingQueueMessage {
    Result(u64, Cvc5BenchmarkRun, Option<GcovRes>),
    Stop,
}

enum ProcessingStatusMessage {
    DbInitSuccess,
    DbInitError,
    BenchDone(u64),
    Benchmarks(Vec<Benchmark>),
}

pub struct Runner {
    runner_workers: Vec<worker::Worker>,
    runner_queue: channel::Sender<RunnerQueueMessage>,

    processing_status_queue: channel::Receiver<ProcessingStatusMessage>,
    processing_worker: worker::Worker,
    processing_queue: channel::Sender<ProcessingQueueMessage>,

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
            processing_queue,
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
            ProcessingStatusMessage::BenchDone(res) => res,
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

    // Due to circular dependency between workers, use this with care, it will crash
    pub fn stop(&mut self) {
        // FIXME: This is a hack
        for _ in &self.runner_workers {
            self.runner_queue.send(RunnerQueueMessage::Stop).unwrap();
        }
        self.processing_queue
            .send(ProcessingQueueMessage::Stop)
            .unwrap();
    }

    pub fn join(&mut self) {
        for runner in &mut self.runner_workers {
            runner.join();
        }

        self.processing_worker.join();
    }
}
