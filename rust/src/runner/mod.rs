mod cvc5;
mod gcov;
mod worker;
use crate::db::Db;
use crate::types::{Benchmark, Status as BenchStatus};
use crate::ARGS;

use std::sync::{mpsc, Arc, Mutex};

enum QueueMessage {
    GcovCmd(Benchmark),
    Cvc5Cmd(Benchmark),
    Stop,
}

pub struct Runner {
    _workers: Vec<worker::Worker>,
    _wqueue: mpsc::Sender<QueueMessage>,
}

impl Runner {
    pub fn new() -> Self {
        let no_workers = ARGS.job_size;

        assert!(no_workers > 0);

        let (wsender, wreceiver) = mpsc::channel();
        let wreceiver = Arc::new(Mutex::new(wreceiver));

        let mut workers = Vec::with_capacity(no_workers);
        for id in 0..no_workers {
            workers.push(worker::Worker::new(id, Arc::clone(&wreceiver)));
        }

        Self {
            _workers: workers,
            _wqueue: wsender,
        }
    }

    pub fn enqueue_gcov(&self, db: &mut Db, benchmark: Benchmark) {
        db.update_benchmark_status(benchmark.id, BenchStatus::Processing)
            .expect("Could not update benchmark status");
        self._wqueue.send(QueueMessage::GcovCmd(benchmark)).unwrap();
    }

    pub fn enqueue_cvc5(&self, db: &mut Db, benchmark: Benchmark) {
        db.update_benchmark_status(benchmark.id, BenchStatus::Running)
            .expect("Could not update benchmark status");
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
