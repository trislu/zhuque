use std::net::IpAddr;

use clap::{Parser, ValueEnum};
use tracing::level_filters::LevelFilter;

#[derive(Clone, Debug, ValueEnum)]
pub(crate) enum Trace {
    Stdout,
    Stderr,
    Tmp,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

impl From<Level> for LevelFilter {
    fn from(value: Level) -> LevelFilter {
        match value {
            Level::Trace => LevelFilter::TRACE,
            Level::Debug => LevelFilter::DEBUG,
            Level::Info => LevelFilter::INFO,
            Level::Warn => LevelFilter::WARN,
            Level::Error => LevelFilter::ERROR,
            Level::Off => LevelFilter::OFF,
        }
    }
}

#[derive(Debug, Clone, Parser)]
#[command(author, about = "zhuque", long_about = None)]
pub(crate) struct Cli {
    #[arg(
        short = 'a',
        long = "addr",
        value_name = "ADDR",
        default_value = "127.0.0.1",
        help = "server address"
    )]
    pub(crate) addr: IpAddr,
    #[arg(
        short = 'p',
        long = "port",
        value_name = "PORT",
        default_value_t = 1965,
        help = "server port"
    )]
    pub(crate) port: u16,
    #[arg(
        short = 'c',
        long = "cert",
        value_name = "FILE",
        required = true,
        help = "cert pem file path"
    )]
    pub(crate) cert: String,
    #[arg(
        short = 'k',
        long = "key",
        value_name = "FILE",
        required = true,
        help = "key pem file path"
    )]
    pub(crate) key: String,
    #[clap(value_enum, short='t', long="trace", default_value_t = Trace::Stderr, help = "trace output")]
    pub(crate) trace: Trace,
    #[clap(value_enum, short='l', long="level", default_value_t = Level::Info, help = "trace output level")]
    pub(crate) level: Level,
}
