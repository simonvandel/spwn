extern crate curl;
extern crate env_logger;
extern crate futures;
extern crate tokio_core;
extern crate tokio_curl;
extern crate threadpool;
#[macro_use]
extern crate clap;

mod args;

use args::{Config, parse_args};

use curl::easy::Easy;
use futures::{Future, lazy};
use futures::future::{ok, join_all};
use tokio_core::reactor::Core;
use tokio_curl::{Perform, Session};
use std::rc::Rc;
use std::cell::RefCell;
use threadpool::ThreadPool;
use std::sync::mpsc::channel;

struct Worker {
    event_loop: Rc<RefCell<Core>>,
}

impl Worker {
    pub fn new(event_loop: Rc<RefCell<Core>>) -> Self {
        Worker { event_loop: event_loop }
    }

    pub fn send_request(&self, url: String) -> Perform {
        let lp = self.event_loop.borrow();

        let session = Session::new(lp.handle());
        let mut a = Easy::new();
        a.get(true).unwrap();
        a.url(&url).unwrap();
        a.write_function(|data| Ok(data.len())).unwrap();
        session.perform(a)
    }
}

/// Delegates and manages all the work.
struct Boss {
    /// Number of requests successfully completed
    pub requests_completed: usize,
    /// Number of active connections
    pub connections: usize,

    thread_pool: ThreadPool,
    num_threads: usize,
}

impl Boss {
    pub fn new(num_threads: usize) -> Self {
        Boss {
            requests_completed: 0,
            connections: 0,
            thread_pool: ThreadPool::new(num_threads),
            num_threads: num_threads,
        }
    }

    /// Should return a future
    pub fn start_workforce(&self, desired_connections: usize, url: String) {

        let jobs = desired_connections;
        let (tx, rx) = channel();
        for _ in 0..self.num_threads {
            let tx = tx.clone();
            let url = url.clone();
            self.thread_pool.execute(move || {
                let lp = Core::new().unwrap();
                let lp = Rc::new(RefCell::new(lp));

                let worker = Worker::new(lp.clone());
                let mut futures = Vec::new();
                for _ in 0..jobs {
                    let url = url.clone();
                    let future = worker.send_request(url)
                        .then(|x| {
                            tx.send(1).unwrap();
                            x
                        });
                    futures.push(future);
                }
                let future = join_all(futures);
                lp.borrow_mut().run(future).unwrap();
            });
        }

        let res = rx.iter().take(jobs * self.num_threads).fold(0, |a, b| a + b);
        println!("{}", res);
    }
}

fn start(config: Config) {
    let boss = Boss::new(config.num_threads);
    boss.start_workforce(config.num_connections, config.url)
}

fn main() {
    env_logger::init().unwrap();
    let config = parse_args();
    start(config);
}
