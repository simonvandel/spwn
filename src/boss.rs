extern crate hyper;

use std::sync::mpsc::{Receiver, channel};
use std::thread;
use args::Config;
use run_info::RunInfo;
use misc::split_number;
use tokio_core::reactor::Core;
use chrono::{Duration, Local, DateTime};
use hyper::{Client, Url};
use std::str::FromStr;
use hyper::client::HttpConnector;

use futures::{Future, stream};
use futures::future::{ok, loop_fn, Loop};
use futures::Stream;
use futures_utils::stopwatch;
use request_result::RequestResult;
use errors::*;

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

    pub fn start_workforce(&self, config: &Config) -> Result<RunInfo> {
        let (tx, rx) = channel();
        let desired_connections_per_worker_iter = split_number(config.num_connections,
                                                               self.num_threads);
        // start num_threads workers
        for desired_connections_per_worker in desired_connections_per_worker_iter {
            let tx = tx.clone();
            let url = config.url.clone();
            let duration = config.duration;
            let timeout = config.timeout.to_std()?;
            let hyper_url = Url::from_str(&url)?;
            let start_time = Local::now();
            let wanted_end_time = start_time + duration;
            thread::spawn(move || {
                // TODO: how to use error_chain on tokio-core?
                let mut core = Core::new().expect("Failed to create Tokio core");
                let hyper_client = hyper::Client::configure()
                    .keep_alive_timeout(Some(timeout))
                    .build(&core.handle());

                let iterator = (0..desired_connections_per_worker).map(|_| {
                    create_looping_worker(duration,
                                          hyper_url.clone(),
                                          &hyper_client,
                                          wanted_end_time)
                });
                let future = stream::futures_unordered(iterator)
                    .fold(RunInfo::new(duration), |mut runinfo_acc, runinfo| {
                        runinfo_acc.merge(&runinfo);
                        ok(runinfo_acc)
                    });
                // TODO: how to use error_chain with tokio?
                let res = core.run(future).expect("Failed to run Tokio Core");

                tx.send(res)
            });
        }

        Ok(self.collect_workers(rx, config.duration))
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

fn create_looping_worker<'a>(duration: Duration,
                             url: Url,
                             hyper_client: &'a Client<HttpConnector>,
                             wanted_end_time: DateTime<Local>)
                             -> impl Future<Item = RunInfo, Error = ()> + 'a {
    let runinfo = RunInfo::new(duration);
    loop_fn((runinfo), move |mut runinfo| {
        let url = url.clone();
        send_request(url, hyper_client).then(move |res| {
            match res {
                // on request success
                Ok(request_res) => {
                    // update histogram
                    request_res.latency
                        .num_nanoseconds()
                        .map(|x| runinfo.histogram.increment(x as u64).ok());
                    runinfo.requests_completed += 1;
                    loop_iter(runinfo, wanted_end_time)
                }
                // on request failure
                Err(_) => {
                    runinfo.num_failed_requests += 1;
                    loop_iter(runinfo, wanted_end_time)
                }
            }
        })
    })
}

/// Determines what the next loop iteration should be; Continue or break.
fn loop_iter(state: RunInfo,
             wanted_end_time: DateTime<Local>)
             -> impl Future<Item = Loop<RunInfo, RunInfo>, Error = ()> {
    let now_time = Local::now();
    if now_time < wanted_end_time {
        ok(Loop::Continue(state))
    } else {
        ok(Loop::Break(state))
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
