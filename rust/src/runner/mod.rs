mod cvc5;
mod gcov;
mod worker;
pub use gcov::GcovRes;

use crate::types::{Benchmark, Cvc5BenchmarkRun};
use crate::ARGS;

use std::collections::HashSet;
use std::process::exit;
use std::sync::{mpsc, Arc, Mutex};

enum RunnerQueueMessage {
    Start(Benchmark),
    Stop,
}

enum ProcessingQueueMessage {
    Start(Benchmark),
    Result(Benchmark, Cvc5BenchmarkRun, Option<GcovRes>),
    Stop,
}

pub struct Runner {
    runner_workers: Vec<worker::Worker>,
    runner_queue: mpsc::Sender<RunnerQueueMessage>,

    processing_worker_ready_queue: mpsc::Receiver<Result<(), ()>>,
    processing_worker: worker::Worker,
    processing_queue: mpsc::Sender<ProcessingQueueMessage>,

    enqueued: Box<HashSet<u64>>,
}

impl Runner {
    pub fn new() -> Self {
        let no_workers = ARGS.job_size;

        assert!(no_workers > 0);

        let (p_ready_send, p_ready_receiver) = mpsc::channel();
        let (p_sender, p_receiver) = mpsc::channel();
        let processing_queue = p_sender;
        let processing_worker = worker::Worker::new_processing(p_ready_send.clone(), p_receiver);

        let (r_sender, r_receiver) = mpsc::channel();
        let runner_receiver = Arc::new(Mutex::new(r_receiver));
        let runner_queue = r_sender;

        let mut runner_workers = Vec::with_capacity(no_workers);
        for id in 0..no_workers {
            runner_workers.push(worker::Worker::new_cmd(
                id,
                Arc::clone(&runner_receiver),
                processing_queue.clone(),
            ));
        }

        Self {
            runner_workers,
            runner_queue,

            processing_worker_ready_queue: p_ready_receiver,
            processing_worker,
            processing_queue,

            enqueued: Box::from(HashSet::new()),
        }
    }

    pub fn wait_on_db_ready(&mut self) {
        match self.processing_worker_ready_queue.recv() {
            Ok(_) => {}
            Err(_) => exit(1),
        }
    }

    pub fn enqueue(&mut self, benchmark: Benchmark) {
        // Safety guard, if DB worker falls behind it happens that
        // we try to enqueue entries multiple times
        if !self.enqueued.contains(&benchmark.id) {
            self.enqueued.insert(benchmark.id);
            self.processing_queue
                .send(ProcessingQueueMessage::Start(benchmark.clone()))
                .unwrap();
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
