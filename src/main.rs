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

#[macro_use]
#[cfg(test)]
extern crate quickcheck;

mod args;
mod misc;

use args::{Config, parse_args};

use curl::easy::Easy;
use futures::{Future, stream};
use futures::future::{ok, loop_fn, Loop};
use futures::Stream;
use tokio_core::reactor::Core;
use tokio_curl::{Session, PerformError};
use chrono::*;
use std::sync::mpsc::channel;
use histogram::*;
use std::thread;

struct RequestResult {
    latency: Duration,
}

impl RequestResult {
    fn new(latency: Duration) -> Self {
        RequestResult { latency: latency }
    }
}

impl Default for RequestResult {
    fn default() -> Self {
        RequestResult { latency: Duration::zero() }
    }
}

fn send_request(easy_request: Easy,
                session: &Session)
                -> impl Future<Item = (RequestResult, Easy), Error = PerformError> {
    session.perform(easy_request)
        .and_then(|mut easy| {
            let latency = easy.total_time()
                .ok()
                .and_then(|x| Duration::from_std(x).ok())
                .unwrap();
            let request_result = RequestResult::new(latency);
            Ok((request_result, easy))
        })
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
    fn new(duration: Duration) -> Self {
        RunInfo {
            requests_completed: 0,
            num_failed_requests: 0,
            duration: duration,
            histogram: Histogram::new(),
        }
    }
    pub fn requests_per_second(&self) -> f32 {
        (self.requests_completed as f32) / (self.duration.num_seconds() as f32)
    }

    fn merge(&mut self, other: &Self) {
        self.requests_completed += other.requests_completed;
        self.num_failed_requests += other.num_failed_requests;
        self.histogram.merge(&other.histogram);
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

fn make_easy(url: &str, timeout: Duration) -> Easy {
    let mut easy_request = Easy::new();
    easy_request.get(true).unwrap();
    easy_request.url(&url).unwrap();
    easy_request.timeout(timeout.to_std().unwrap()).unwrap();
    easy_request
}

impl Boss {
    pub fn new(num_threads: usize) -> Self {
        Boss {
            requests_completed: 0,
            connections: 0,
            num_threads: num_threads,
        }
    }

    pub fn start_workforce(&self, config: &Config) -> RunInfo {
        let (tx, rx) = channel();
        let desired_connections_per_worker_iter = misc::split_number(config.num_connections,
                                                                     self.num_threads);
        // start num_threads workers
        for desired_connections_per_worker in desired_connections_per_worker_iter {
            let tx = tx.clone();
            let url = config.url.clone();
            let duration = config.duration;
            let timeout = config.timeout;
            thread::spawn(move || {

                let mut lp = Core::new().unwrap();
                let start_time = Local::now();
                let wanted_end_time = start_time + duration;
                let session = Session::new(lp.handle());

                let iterator = (0..desired_connections_per_worker).map(|_| {

                    let runinfo = RunInfo::new(duration);
                    let easy_request = make_easy(&url, timeout);
                    loop_fn((runinfo, easy_request), |(mut runinfo, easy_request)| {
                        send_request(easy_request, &session)
                            .then(|res| -> Result<_, ()> { match res {
                                // on request success
                                Ok((request_res, easy_request)) => {
                                    // update histogram
                                    request_res.latency.num_nanoseconds()
                                        .map(|x| runinfo.histogram.increment(x as u64).ok());
                                    runinfo.requests_completed += 1;

                                    let state = (runinfo, easy_request);

                                    let now_time = Local::now();
                                    if now_time < wanted_end_time {
                                        Ok(Loop::Continue(state))
                                    } else {
                                        Ok(Loop::Break(state))
                                    }
                                }
                                // on request failure
                                Err(mut err) => {
                                    runinfo.num_failed_requests += 1;
                                    // attempt to recover the easy handle from the error,
                                    // else make a new handle
                                    let easy_request = err.take_easy().unwrap_or(make_easy(&url, timeout));
                                    let state = (runinfo, easy_request);
                                    let now_time = Local::now();
                                    if now_time < wanted_end_time {
                                        Ok(Loop::Continue(state))
                                    } else {
                                        Ok(Loop::Break(state))
                                    }
                                }
                            }})
                            .map_err(|_| ())
                    })
                        // Extract latency histogram
                        .map(|(histogram, _)| histogram)
                });
                let future = stream::futures_unordered(iterator)
                    .fold(RunInfo::new(duration), |mut runinfo_acc, runinfo| {
                        runinfo_acc.merge(&runinfo);
                        ok(runinfo_acc)
                    });
                let res = lp.run(future).unwrap();
                tx.send(res).unwrap();
            });
        }
        // collect information from all workers
        rx.iter()
            .take(self.num_threads)
            .fold(RunInfo::new(config.duration), |mut runinfo_acc, runinfo| {
                runinfo_acc.merge(&runinfo);
                runinfo_acc
            })
    }
}

fn start(config: Config) -> RunInfo {
    let boss = Boss::new(config.num_threads);
    boss.start_workforce(&config)
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

    // we can only print the latency distribution if there is data in the histogram
    if run_info.histogram.entries() > 0 {
        println!("Latency distribution: \n50%: {} ms\n75%: {} ms\n90%: {} ms\n95%: {} ms\n99%: {} ms",
            run_info.histogram.percentile(50.0).map(nanoseconds_to_milliseconds).unwrap(),
            run_info.histogram.percentile(75.0).map(nanoseconds_to_milliseconds).unwrap(),
            run_info.histogram.percentile(90.0).map(nanoseconds_to_milliseconds).unwrap(),
            run_info.histogram.percentile(95.0).map(nanoseconds_to_milliseconds).unwrap(),
            run_info.histogram.percentile(99.0).map(nanoseconds_to_milliseconds).unwrap(),
        );
    }

}

fn main() {
    env_logger::init().unwrap();
    let config = parse_args();
    let run_info = start(config);
    present(run_info)
}
