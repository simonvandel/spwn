#![feature(conservative_impl_trait)]

// `error_chain!` can recurse deeply
#![recursion_limit = "1024"]

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
extern crate error_chain;

#[macro_use]
#[cfg(test)]
extern crate quickcheck;

extern crate time;
extern crate log;

extern crate dns_lookup;

mod args;
mod misc;
mod futures_utils;
mod request_result;
mod run_info;
mod boss;
mod errors;

use args::{Config, parse_args};
use misc::nanoseconds_to_milliseconds;
use run_info::RunInfo;
use boss::Boss;
use errors::*;

fn start(config: Config) -> Result<RunInfo> {
    let boss = Boss::new(config.num_threads);
    boss.start_workforce(&config)
}

/// Presents the results to the user
fn present(run_info: RunInfo) -> Result<()> {
    let requests_completed = run_info.requests_completed;
    println!("{} requests in {}s",
             requests_completed,
             run_info.duration.num_seconds());
    println!("{} failed requests", run_info.num_failed_requests);
    println!("Requests per second: {}", run_info.requests_per_second());

    // we can only print the latency distribution if there is data in the histogram
    if run_info.histogram.entries() > 0 {
        println!("Average latency: {}ms",
                 run_info.histogram.mean().map(nanoseconds_to_milliseconds)?);
        println!("Latency distribution: \n50%: {} ms\n75%: {} ms\n90%: {} ms\n95%: {} ms\n99%: {} ms",
            run_info.histogram.percentile(50.0).map(nanoseconds_to_milliseconds)?,
            run_info.histogram.percentile(75.0).map(nanoseconds_to_milliseconds)?,
            run_info.histogram.percentile(90.0).map(nanoseconds_to_milliseconds)?,
            run_info.histogram.percentile(95.0).map(nanoseconds_to_milliseconds)?,
            run_info.histogram.percentile(99.0).map(nanoseconds_to_milliseconds)?,
        );
    };

    Ok(())
}

fn run() -> errors::Result<()> {
    env_logger::init()?;
    let config = parse_args();
    let run_info = start(config)?;
    present(run_info)
}

fn main() {
    if let Err(ref e) = run() {
        use std::io::Write;
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "error: {}", e).expect(errmsg);

        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).expect(errmsg);
        }

        // The backtrace is not always generated. Try to run this example
        // with `RUST_BACKTRACE=1`.
        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace).expect(errmsg);
        }

        ::std::process::exit(1);
    }

}
