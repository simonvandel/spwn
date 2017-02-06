#![feature(conservative_impl_trait)]

extern crate curl;
extern crate env_logger;
extern crate futures;
extern crate tokio_core;
extern crate tokio_curl;
#[macro_use]
extern crate clap;
extern crate chrono;
extern crate histogram;

mod args;

use args::{Config, parse_args};

use curl::easy::Easy;
use futures::{Future, stream};
use futures::future::{ok, loop_fn, Loop};
use futures::Stream;
use tokio_core::reactor::Core;
use tokio_curl::{PerformError, Session};
use chrono::*;
use std::sync::mpsc::channel;
use histogram::*;
use std::thread;

struct RequestResult {
    /// The worker that processed the request
    worker: Worker,
    /// The curl easy response to the request
    curl_easy: Easy,
}

impl RequestResult {
    fn new(worker: Worker, curl_easy: Easy) -> Self {
        RequestResult {
            worker: worker,
            curl_easy: curl_easy,
        }
    }
}

struct Worker {
    /// Number of succesful requests this worker has made
    num_requests: usize,
    histogram: Histogram,
    /// Number of failed requests this worker has made
    num_failed_requests: usize,
}

impl Worker {
    pub fn new() -> Self {
        Worker {
            num_requests: 0,
            histogram: Histogram::new(),
            num_failed_requests: 0,
        }
    }

    fn merge(&mut self, other: &Self) {
        self.num_requests += other.num_requests;
        self.num_failed_requests += other.num_failed_requests;
        self.histogram.merge(&other.histogram);
    }

    fn send_request(mut self, easy_request: Easy, session: &Session) -> impl Future<Item=RequestResult, Error=PerformError> {
        session.perform(easy_request)
            .then(|res| {
                match res {
                    Ok(mut easy) => {
                        self.num_requests += 1;
                        easy.total_time()
                            .ok()
                            .and_then(|x| Duration::from_std(x).ok())
                            .and_then(|latency| {
                                latency.num_nanoseconds()
                                    .map(|x| self.histogram.increment(x as u64).ok())
                            });
                        let request_result = RequestResult::new(self, easy);
                        Ok(request_result)
                    }
                    Err(error) => {
                        self.num_failed_requests += 1;
                        Err(error)
                    }
                }
            })
    }
}

struct RunInfo {
    /// Number of requests successfully completed
    pub requests_completed: usize,
    pub duration: Duration,
    histogram: Histogram,
    /// Number of failed requests
    num_failed_requests: usize,
}

impl RunInfo {
    pub fn requests_per_second(&self) -> f32 {
        (self.requests_completed as f32) / (self.duration.num_seconds() as f32)
    }
}

/// Delegates and manages all the work.
struct Boss {
    /// Number of requests successfully completed
    pub requests_completed: usize,
    /// Number of active connections
    pub connections: usize,
    num_threads: usize,
}

impl Boss {
    pub fn new(num_threads: usize) -> Self {
        Boss {
            requests_completed: 0,
            connections: 0,
            num_threads: num_threads,
        }
    }

    pub fn start_workforce(&self,
                           desired_connections: usize,
                           url: String,
                           duration: Duration)
                           -> RunInfo {
        let (tx, rx) = channel();
        for _ in 0..self.num_threads {
            let tx = tx.clone();
            let url = url.clone();
            thread::spawn(move || {

                let mut lp = Core::new().unwrap();
                let start_time = Local::now();
                let wanted_end_time = start_time + duration;
                let session = Session::new(lp.handle());

                let iterator = (0..desired_connections).map(|_| {
                    let mut easy_request = Easy::new();
                    easy_request.get(true).unwrap();
                    easy_request.url(&url).unwrap();
                    let request_result = RequestResult::new(Worker::new(), easy_request);
                    loop_fn(request_result, |request_result| {
                        request_result.worker.send_request(request_result.curl_easy, &session)
                            .and_then(|state| {
                                let now_time = Local::now();
                                if now_time < wanted_end_time {
                                    Ok(Loop::Continue(state))
                                } else {
                                    Ok(Loop::Break(state))
                                }
                            })
                    })
                        // Extract worker statistics
                        .map(|request_res| request_res.worker)
                });
                let future = stream::futures_unordered(iterator)
                    .fold(Worker::new(), |mut worker_acc, worker| {
                        worker_acc.merge(&worker);
                        ok(worker_acc)
                    });
                let res = lp.run(future).unwrap();
                tx.send(res).unwrap();
            });
        }
        // collect information from all workers
        let worker_info: Worker = rx.iter()
            .take(self.num_threads)
            .fold(Worker::new(), |mut worker_acc, worker| {
                worker_acc.merge(&worker);
                worker_acc
            });
        RunInfo {
            requests_completed: worker_info.num_requests,
            num_failed_requests: worker_info.num_failed_requests,
            duration: duration,
            histogram: worker_info.histogram,
        }
    }
}

fn start(config: Config) -> RunInfo {
    let boss = Boss::new(config.num_threads);
    boss.start_workforce(config.num_connections, config.url, config.duration)
}

fn nanoseconds_to_milliseconds(nanoseconds: u64) -> f64 {
    nanoseconds as f64 / 1_000_000_f64
}

/// Presents the results to the user
fn present(run_info: RunInfo) {
    let requests_completed = run_info.requests_completed;
    println!("{} requests in {}s",
             requests_completed,
             run_info.duration.num_seconds());
    println!("{} failed requests", run_info.num_failed_requests);
    println!("Requests per second: {}", run_info.requests_per_second());
    println!("Latency distribution: \n50%: {} ms\n75%: {} ms\n90%: {} ms\n95%: {} ms\n99%: {} ms",
        run_info.histogram.percentile(50.0).map(nanoseconds_to_milliseconds).unwrap(),
        run_info.histogram.percentile(75.0).map(nanoseconds_to_milliseconds).unwrap(),
        run_info.histogram.percentile(90.0).map(nanoseconds_to_milliseconds).unwrap(),
        run_info.histogram.percentile(95.0).map(nanoseconds_to_milliseconds).unwrap(),
        run_info.histogram.percentile(99.0).map(nanoseconds_to_milliseconds).unwrap(),
    );
}

fn main() {
    env_logger::init().unwrap();
    let config = parse_args();
    let run_info = start(config);
    present(run_info)
}
