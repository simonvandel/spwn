extern crate hyper;

use std::sync::mpsc::{Receiver, channel, Sender};
use std::thread;
use args::Config;
use run_info::RunInfo;
use misc::split_number;
use tokio_core::reactor::Core;
use chrono::{Duration, Local};
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
            let timeout = config.timeout.to_std()?;
            let hyper_url = Url::from_str(&url)?;
            thread::spawn(move || {
                // TODO: how to use error_chain on tokio-core?
                let mut core = Core::new().expect("Failed to create Tokio core");
                let hyper_client = hyper::Client::configure()
                    .keep_alive_timeout(Some(timeout))
                    .build(&core.handle());

                let iterator = (0..desired_connections_per_worker).map(|_| {
                    let tx = tx.clone();
                    
                    create_looping_worker(hyper_url.clone(), &hyper_client)
                        .for_each(move |req| {
                            tx.send(req);
                            ok(())
                        })
                });
                // create stream of requests
                let request_stream = stream::futures_unordered(iterator);
                // TODO: how to use error_chain with tokio?
                let _ = core.run(request_stream.into_future());
            });
        }

        Ok(self.collect_workers(rx, config.duration))
    }

    /// Collects information from all workers
    fn collect_workers(&self, rx: Receiver<RequestResult>, run_duration: Duration) -> RunInfo {
        let start_time = Local::now();
        let wanted_end_time = start_time + run_duration;
        rx.iter()
            .take_while(|_| Local::now() < wanted_end_time)
            .fold(RunInfo::new(run_duration), |mut runinfo_acc, request_result| {
                runinfo_acc.add_request(&request_result);
                runinfo_acc
            })
    }
}

fn create_looping_worker<'a>(url: Url,
                             hyper_client: &'a Client<HttpConnector>
                             )
                             -> impl Stream<Item = RequestResult, Error = ()> + 'a {
    stream::unfold(0u32, move |state| {
        let res = send_request(url.clone(), hyper_client);
        
        let fut = res.and_then(move |req| ok::<_, _>((req, state)));
        Some(fut)
    })
}

fn send_request<C>(url: Url,
                   hyper_client: &Client<C>)
                   -> impl Future<Item = RequestResult, Error = ()>
    where C: hyper::client::Connect
{
    stopwatch(hyper_client.get(url))
        .and_then(|(_, latency)| {
            let request_result = RequestResult::Success { latency: latency };
            Ok(request_result)
        })
        .or_else(|_| ok(RequestResult::Failure))
}
