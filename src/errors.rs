use time::OutOfRangeError;
use log;

error_chain! {

    foreign_links {
        TimeOutOfRange(OutOfRangeError);
        LoggerError(log::SetLoggerError);
    }
 }
