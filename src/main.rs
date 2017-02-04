extern crate curl;
extern crate env_logger;
extern crate futures;
extern crate tokio_core;
extern crate tokio_curl;
extern crate threadpool;
#[macro_use]
extern crate clap;
extern crate chrono;

mod args;

use args::{Config, parse_args};

use curl::easy::Easy;
use futures::{Async, Future, lazy, Poll, stream};
use futures::future::{ok, join_all, FutureResult, loop_fn, Loop};
use futures::sync::mpsc::channel;
use futures::{Stream, Sink};
use tokio_core::reactor::{Core, Handle};
use tokio_curl::{Perform, PerformError, Session};
use std::rc::Rc;
use std::cell::RefCell;
use threadpool::ThreadPool;
use std::sync::{Arc, Barrier};
use futures::stream::FuturesUnordered;
use chrono::*;


struct Worker {
    num_requests: u32,
}

impl Worker {
    pub fn new() -> Self {
        Worker { num_requests: 0}
    }

    fn send_request(mut self, url: String, session: &Session) -> futures::BoxFuture<Self, PerformError> {
        let mut a = Easy::new();
        a.get(true).unwrap();
        a.url(&url).unwrap();
        a.write_function(|data| Ok(data.len())).unwrap();
        Box::new(session.perform(a)
            .and_then(move |_| {
                self.num_requests += 1;
                ok(self)
            }))
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
        // create a barrier that wait all jobs plus the starter thread
        let barrier = Arc::new(Barrier::new(self.num_threads + 1));
        for _ in 0..self.num_threads {
            let barrier = barrier.clone();
            // let tx = tx.clone();
            let url = url.clone();
            self.thread_pool.execute(move || {
                
                let mut lp = Core::new().unwrap();
                let start_time = Local::now();
                let wanted_end_time = start_time + Duration::seconds(10);
                let session = Session::new(lp.handle());

                let iterator = (0..desired_connections).map(|_| {
                    loop_fn(Worker::new(), |worker| {
                        worker.send_request(url.clone(), &session)
                            .and_then(|count| {
                                let now_time = Local::now();
                                if now_time < wanted_end_time {
                                    Ok(Loop::Continue(count))
                                } else {
                                    Ok(Loop::Break(count))
                                }
                            })
                    })
                });
                let future = stream::futures_unordered(iterator)
                    .fold(0, |acc, res| ok(acc + res.num_requests));
                let res = lp.run(future).unwrap();
                println!("{:?}", res);

                // then wait for the other threads
                barrier.wait();
            });
        }
        // wait for the threads to finish the work
        barrier.wait();
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
