use stopwatch::Stopwatch;
use futures::Future;
use chrono::Duration;

pub fn stopwatch<F, I, E>(future: F) -> impl Future<Item = (I, Duration), Error = E>
    where F: Future<Item = I, Error = E>
{
    let sw = Stopwatch::start_new();
    future.then(move |res| {
        res.map(move |x| {
            (x, Duration::from_std(sw.elapsed()).expect("Could not convert latency from std time"))
        })
    })
}
