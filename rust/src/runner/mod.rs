mod cvc5;
mod gcov;
mod worker;
pub use gcov::GcovRes;

use crate::types::{Benchmark, BenchmarkRun};
use crate::ARGS;

use std::collections::HashSet;
use std::sync::{mpsc, Arc, Mutex};

enum RunnerQueueMessage {
    Cvc5Cmd(Benchmark),
    GcovCmd(Benchmark),
    Stop,
}

enum ProcessingQueueMessage {
    Cvc5Start(u64),
    GcovStart(u64),
    Cvc5Res(Benchmark, BenchmarkRun),
    GcovRes(Benchmark, GcovRes),
    Stop,
}

pub struct Runner {
    runner_workers: Vec<worker::Worker>,
    runner_queue: mpsc::Sender<RunnerQueueMessage>,

    processing_worker: worker::Worker,
    processing_queue: Arc<Mutex<mpsc::Sender<ProcessingQueueMessage>>>,

    cvc5_enqueued: Box<HashSet<u64>>,
    gcov_enqueued: Box<HashSet<u64>>,
}

impl Runner {
    pub fn new() -> Self {
        let no_workers = ARGS.job_size;

        assert!(no_workers > 0);

        let (p_sender, p_receiver) = mpsc::channel();
        let processing_queue = Arc::new(Mutex::new(p_sender));
        let processing_worker = worker::Worker::new_processing(p_receiver);

        let (r_sender, r_receiver) = mpsc::channel();
        let runner_receiver = Arc::new(Mutex::new(r_receiver));
        let runner_queue = r_sender;

        let mut runner_workers = Vec::with_capacity(no_workers);
        for id in 0..no_workers {
            runner_workers.push(worker::Worker::new_cmd(
                id,
                Arc::clone(&runner_receiver),
                Arc::clone(&processing_queue),
            ));
        }

        Self {
            runner_workers,
            runner_queue,

            processing_worker,
            processing_queue,

            cvc5_enqueued: Box::from(HashSet::new()),
            gcov_enqueued: Box::from(HashSet::new()),
        }
    }

    pub fn enqueue_gcov(&mut self, benchmark: Benchmark) {
        // Safety guard, if DB worker falls behind it happens that
        // we try to enqueue entries multiple times
        if !self.gcov_enqueued.contains(&benchmark.id) {
            self.gcov_enqueued.insert(benchmark.id);
            self.processing_queue
                .lock()
                .unwrap()
                .send(ProcessingQueueMessage::GcovStart(benchmark.id))
                .unwrap();
            self.runner_queue
                .send(RunnerQueueMessage::GcovCmd(benchmark))
                .unwrap();
        }
    }

    pub fn enqueue_cvc5(&mut self, benchmark: Benchmark) {
        // Safety guard, if DB worker falls behind it happens that
        // we try to enqueue entries multiple times
        if !self.cvc5_enqueued.contains(&benchmark.id) {
            self.cvc5_enqueued.insert(benchmark.id);
            self.processing_queue
                .lock()
                .unwrap()
                .send(ProcessingQueueMessage::Cvc5Start(benchmark.id))
                .unwrap();
            self.runner_queue
                .send(RunnerQueueMessage::Cvc5Cmd(benchmark))
                .unwrap();
        }
    }

    pub fn stop(&mut self) {
        self.runner_queue.send(RunnerQueueMessage::Stop).unwrap();
        self.processing_queue
            .lock()
            .unwrap()
            .send(ProcessingQueueMessage::Stop)
            .unwrap();
    }

    pub fn join(&mut self) {
        for worker in &mut self.runner_workers {
            worker.join()
        }
        self.processing_worker.join()
    }
}
