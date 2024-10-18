use super::cvc5;
use super::gcov;
use super::QueueMessage;
use crate::db::Db;
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
    // FIXME: Don't have multiple writers, this will cause issues.
    // Send the data back to a single writer thread and write things from there

    pub(super) fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<QueueMessage>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            let mut db = Db::new().expect("Could not connect to the DB in worker");
            let cvc5cmd = Path::join(&ARGS.build_dir, "bin/cvc5");

            let job = receiver.lock().unwrap().recv();
            match job {
                Ok(QueueMessage::Cvc5Cmd(benchmark)) => {
                    info!("Worker {} got a job; executing.", id);
                    cvc5::process(&mut db, &cvc5cmd, &benchmark)
                }
                Ok(QueueMessage::GcovCmd(benchmark)) => {
                    info!("Worker {} got a job; executing.", id);
                    gcov::process(&mut db, &benchmark)
                }
                Ok(QueueMessage::Stop) => {
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
