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

                // // create infinite stream of requests by unfolding
                // let request_stream = stream::unfold(0u32, |state| {
                //     let fut = send_request(hyper_url.clone(), &hyper_client);
                //     // create a wrapper around the future, so we can pass the state.
                //     // in this case, we do not need the state, so just pass it on unmodified
                //     let fut_wrapper = fut.map(move |request_result| (request_result, state));
                //     Some(fut_wrapper)
                // });


                
                // TODO: make infinite
                let iterator = (0..desired_connections_per_worker).map(|_| {
                    create_looping_worker(duration,
                                          hyper_url.clone(),
                                          &hyper_client,
                                          wanted_end_time)
                });
                // create stream of requests
                let request_stream = stream::futures_unordered(iterator);
                let future = request_stream
                    .for_each(|request_result| {
                        let _ = tx.send(request_result);
                        Ok(())
                    });
                // TODO: how to use error_chain with tokio?
                let _ = core.run(future).expect("Failed to run Tokio Core");
            });
        }

        Ok(self.collect_workers(rx, config.duration))
    }

    /// Collects information from all workers
    fn collect_workers(&self, rx: Receiver<RunInfo>, run_duration: Duration) -> RunInfo {
        let start_time = Local::now();
        let wanted_end_time = start_time + run_duration;
        rx.iter()
            .take(100)
            // .take_while(|_| Local::now() < wanted_end_time)
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
                    ok(loop_iter(runinfo, wanted_end_time))
                }
                // on request failure
                Err(_) => {
                    runinfo.num_failed_requests += 1;
                    ok(loop_iter(runinfo, wanted_end_time))
                }
            }
        })
    })
}

/// Determines what the next loop iteration should be; Continue or break.
fn loop_iter(state: RunInfo,
             wanted_end_time: DateTime<Local>)
             -> Loop<RunInfo, RunInfo> {
    let now_time = Local::now();
    if now_time < wanted_end_time {
        Loop::Continue(state)
    } else {
        Loop::Break(state)
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
