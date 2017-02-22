#![feature(conservative_impl_trait)]

extern crate env_logger;
extern crate futures;
extern crate tokio_core;

#[macro_use]
extern crate clap;
extern crate chrono;
extern crate histogram;
extern crate hyper;
extern crate stopwatch;


#[macro_use]
#[cfg(test)]
extern crate quickcheck;

mod args;
mod misc;
mod futures_utils;
mod request_result;

use args::{Config, parse_args};

use futures::{Future, stream};
use futures::future::{ok, loop_fn, Loop};
use futures::Stream;
use tokio_core::reactor::Core;
use chrono::*;
use std::sync::mpsc::channel;
use histogram::*;
use std::thread;
use hyper::{Client, Url};
use std::str::FromStr;
use futures_utils::stopwatch;
use request_result::RequestResult;
use misc::nanoseconds_to_milliseconds;

fn send_request<C>(url: Url,
                   hyper_client: &Client<C>)
                   -> impl Future<Item = RequestResult, Error = hyper::Error>
    where C: hyper::client::Connect
{
    stopwatch(hyper_client.get(url)).and_then(|(_, latency)| {
        let request_result = RequestResult::new(latency);
        Ok(request_result)
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
            let timeout = config.timeout.to_std().unwrap();
            thread::spawn(move || {

                let mut lp = Core::new().unwrap();
                let start_time = Local::now();
                let wanted_end_time = start_time + duration;
                let hyper_client = hyper::Client::configure()
                    .keep_alive_timeout(Some(timeout))
                    .build(&lp.handle());
                let hyper_url = Url::from_str(&url).expect("Invalid URL");

                let iterator = (0..desired_connections_per_worker).map(|_| {

                    let runinfo = RunInfo::new(duration);

                    loop_fn((runinfo), |mut runinfo| {
                        send_request(hyper_url.clone(), &hyper_client)
                            .then(|res| -> Result<_, ()> { match res {
                                // on request success
                                Ok(request_res) => {
                                    // update histogram
                                    request_res.latency.num_nanoseconds()
                                        .map(|x| runinfo.histogram.increment(x as u64).ok());
                                    runinfo.requests_completed += 1;

                                    let state = runinfo;

                                    let now_time = Local::now();
                                    if now_time < wanted_end_time {
                                        Ok(Loop::Continue(state))
                                    } else {
                                        Ok(Loop::Break(state))
                                    }
                                }
                                // on request failure
                                Err(_) => {
                                    runinfo.num_failed_requests += 1;
                                    let state = runinfo;
                                    let now_time = Local::now();
                                    if now_time < wanted_end_time {
                                        Ok(Loop::Continue(state))
                                    } else {
                                        Ok(Loop::Break(state))
                                    }
                                }
                            }})
                    })
                        // Extract latency histogram
                        .map(|histogram| histogram)
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
        println!("Average latency: {}ms",
                 run_info.histogram.mean().map(nanoseconds_to_milliseconds).unwrap());
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
