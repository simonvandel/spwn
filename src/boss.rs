extern crate hyper;

use std::sync::mpsc::{Receiver, channel};
use std::thread;
use args::Config;
use run_info::RunInfo;
use misc::split_number;
use tokio_core::reactor::Core;
use chrono::{Duration, Local};
use hyper::{Client, Url};
use std::str::FromStr;

use futures::{Future, stream};
use futures::future::{ok, loop_fn, Loop};
use futures::Stream;
use futures_utils::stopwatch;
use request_result::RequestResult;

/// Delegates and manages all the work.
pub struct Boss {
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
        let desired_connections_per_worker_iter = split_number(config.num_connections,
                                                                     self.num_threads);
        // start num_threads workers
        for desired_connections_per_worker in desired_connections_per_worker_iter {
            let tx = tx.clone();
            let url = config.url.clone();
            let duration = config.duration;
            let timeout = config.timeout.to_std().unwrap();
            thread::spawn(move || {

                let mut core = Core::new().unwrap();
                let start_time = Local::now();
                let wanted_end_time = start_time + duration;
                let hyper_client = hyper::Client::configure()
                    .keep_alive_timeout(Some(timeout))
                    .build(&core.handle());
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
                });
                let future = stream::futures_unordered(iterator)
                    .fold(RunInfo::new(duration), |mut runinfo_acc, runinfo| {
                        runinfo_acc.merge(&runinfo);
                        ok(runinfo_acc)
                    });
                let res = core.run(future).unwrap();
                tx.send(res).unwrap();
            });
        }
        
        self.collect_workers(rx, config.duration)
    }

    /// Collects information from all workers
    fn collect_workers(&self, rx: Receiver<RunInfo>, run_duration: Duration) -> RunInfo {
        rx.iter()
                .take(self.num_threads)
                .fold(RunInfo::new(run_duration), |mut runinfo_acc, runinfo| {
                    runinfo_acc.merge(&runinfo);
                    runinfo_acc
                })
    }
}

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