use time::OutOfRangeError;
use hyper;
use log;

error_chain! {

    foreign_links {
        TimeOutOfRange(OutOfRangeError);
        LoggerError(log::SetLoggerError);
        UriError(hyper::error::UriError);
    }
 }
