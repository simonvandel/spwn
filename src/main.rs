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
mod run_info;
mod boss;

use args::{Config, parse_args};
use misc::nanoseconds_to_milliseconds;
use run_info::RunInfo;
use boss::Boss;

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
