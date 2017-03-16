use chrono::Duration;
use histogram::Histogram;
use request_result::RequestResult;

pub struct RunInfo {
    /// Number of requests successfully completed
    pub requests_completed: usize,
    pub duration: Duration,
    pub histogram: Histogram,
    /// Number of failed requests
    pub num_failed_requests: usize,
}

impl RunInfo {
    pub fn new(duration: Duration) -> Self {
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

    pub fn add_request(&mut self, request: &RequestResult) {
        use request_result::RequestResult::*;
        match *request {
            Success { latency } => {
                self.requests_completed += 1;
                // update histogram
                latency.num_nanoseconds()
                    .map(|x| self.histogram.increment(x as u64).ok());
            }
            Failure => {
                self.num_failed_requests += 1;
            }
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.requests_completed += other.requests_completed;
        self.num_failed_requests += other.num_failed_requests;
        self.histogram.merge(&other.histogram);
    }
}
