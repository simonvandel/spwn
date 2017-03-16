use time::OutOfRangeError;
use hyper;
use log;

error_chain! {

    foreign_links {
        TimeOutOfRange(OutOfRangeError);
        UrlParseError(hyper::error::ParseError);
        LoggerError(log::SetLoggerError);
    }
 }
