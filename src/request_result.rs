use chrono::Duration;

pub struct RequestResult {
    pub latency: Duration,
}

impl RequestResult {
    pub fn new(latency: Duration) -> Self {
        RequestResult { latency: latency }
    }
}

impl Default for RequestResult {
    fn default() -> Self {
        RequestResult { latency: Duration::zero() }
    }
}
