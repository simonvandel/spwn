use chrono::Duration;

pub enum RequestResult {
    Success { latency: Duration },
    Failure,
}
