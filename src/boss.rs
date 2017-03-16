extern crate hyper;

use futures::sync::mpsc::{unbounded, UnboundedReceiver};
use std::thread;
use args::Config;
use run_info::RunInfo;
use misc::split_number;
use tokio_core::reactor::Core;
use chrono::{Local};
use hyper::{Client, Url};
use std::str::FromStr;
use hyper::client::HttpConnector;

use futures::{Future, stream};
use futures::future::ok;
use futures::Stream;
use futures_utils::stopwatch;
use request_result::RequestResult;
use errors::*;

use dns_lookup::lookup_host;

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
        let (tx, rx) = unbounded();
        let desired_connections_per_worker_iter = split_number(config.num_connections,
                                                               self.num_threads);

        // resolve dns once and for all
        let resolved_url = resolve_dns(&config.url)?;
        let start_time = Local::now();
        let wanted_end_time = start_time + config.duration;
        let run_duration = config.duration;

        // start num_threads workers
        for desired_connections_per_worker in desired_connections_per_worker_iter {
            let tx = tx.clone();
            let timeout = config.timeout.to_std()?;
            let hyper_url = resolved_url.clone();
            thread::spawn(move || {

                // TODO: how to use error_chain on tokio-core?
                let mut core = Core::new().expect("Failed to create Tokio core");
                let hyper_client = hyper::Client::configure()
                    .keep_alive_timeout(Some(timeout))
                    .build(&core.handle());

                let iterator = (0..desired_connections_per_worker).map(|_| {


                    create_looping_worker(hyper_url.clone(), &hyper_client)
                        .take_while(|_| ok(Local::now() < wanted_end_time))
                        .fold(RunInfo::new(run_duration),
                              |mut runinfo_acc, request_result| {
                                  runinfo_acc.add_request(&request_result);
                                  ok(runinfo_acc)
                              })
                });
                // create stream of requests
                let request_stream = stream::futures_unordered(iterator)
                    .for_each(move |run_info| {
                        let tx = tx.clone();
                        let _ = tx.send(run_info);
                        ok(())
                    });
                // TODO: how to use error_chain with tokio?
                let _ = core.run(request_stream);
            });
        }

        Ok(self.collect_workers(rx, config))
    }

    /// Collects information from all workers
    fn collect_workers(&self, rx: UnboundedReceiver<RunInfo>, config: &Config) -> RunInfo {
        rx.take(config.num_connections as u64)
            .fold(RunInfo::new(config.duration), |mut runinfo_acc, run_info| {
                runinfo_acc.merge(&run_info);
                ok(runinfo_acc)
            })
            .wait()
            .unwrap()
    }
}

// resolves the given url to an ip
fn resolve_dns(url: &str) -> Result<Url> {
    let parsed_url = Url::from_str(url)?;
    let host = parsed_url.host_str().unwrap();
    let host = lookup_host(&host)
        .expect("DNS lookup failed")
        .filter_map(|x| x.ok())
        .filter(|x| x.is_ipv4())
        .next()
        .expect("DNS lookup failed");

    let mut parsed_url = parsed_url.clone();
    let _ = parsed_url.set_ip_host(host);
    Ok(parsed_url)
}

fn create_looping_worker<'a>(url: Url,
                             hyper_client: &'a Client<HttpConnector>)
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
