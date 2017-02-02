extern crate num_cpus;
use clap::Arg;

pub struct Config {
    pub url: String,
    pub num_threads: usize,
    pub num_connections: usize,
}

fn integer_validator<E>(input: String, error_message: E) -> Result<(), String>
    where E: Into<String>
{
    input.parse::<usize>()
        .map(|_| ())
        .map_err(|_| error_message.into())
}

pub fn parse_args() -> Config {
    let app = app_from_crate!();
    let num_cpus_str = num_cpus::get().to_string();
    let matches = app.arg(Arg::with_name("url")
            .required(true)
            .help("target url"))
        .arg(Arg::with_name("threads")
            .short("t")
            .default_value(&num_cpus_str)
            .validator(|i| integer_validator(i, "Number of threads must be an integer"))
            .help("Number of threads to use. Default is the number of CPUs"))
        .arg(Arg::with_name("connections")
            .short("c")
            .default_value("10")
            .validator(|i| integer_validator(i, "Number of connections must be an integer"))
            .help("Number of connections to spawn"))
        .get_matches();

    // this can be unwrapped, as the validator should guarantee the argument is an integer
    let num_threads = matches.value_of("threads").map(|x| x.parse()).unwrap().unwrap();
    // this can be unwrapped, as the validator should guarantee the argument is an integer
    let num_connections = matches.value_of("connections").map(|x| x.parse()).unwrap().unwrap();

    Config {
        url: matches.value_of("url").unwrap().into(),
        num_threads: num_threads,
        num_connections: num_connections,
    }
}

