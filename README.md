# Spwn
An HTTP benchmarking program inspired by [wrk](https://github.com/wg/wrk) and [Siege](https://github.com/JoeDog/siege).

[![Build Status](https://travis-ci.org/simonvandel/spwn.svg?branch=master)](https://travis-ci.org/simonvandel/spwn)
[![Build status](https://ci.appveyor.com/api/projects/status/rk5sw7vr3jisu3bw?svg=true)](https://ci.appveyor.com/project/simonvandel/spwn)

## My motivation
I primarily developed this tool to play with [Rust](https://www.rust-lang.org) and [Tokio](https://tokio.rs/).

## Features
- User-configurable number of concurrent connections
- User-configurable number of threads to use
- User-configurable duration to run the benchmark
- Request count (total and per second)
- Latency distribution

## Example
The following example will fire requests, maintaining 100 connections, to `localhost:8080` for 2 seconds using 4 threads.
```sh
spwn localhost:8080 -d2 -c100 -t4
```

## Building
Rust Nightly 1.17 (2017-02-03) is tested.

The following will build an optimized binary.
```sh
cargo build --release
```