use std::net::IpAddr;

use clap::{Parser, ValueEnum};

#[derive(Clone, Debug, ValueEnum)]
pub(crate) enum Trace {
    Stdout,
    Stderr,
    Tmp,
}

#[derive(Debug, Parser)]
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
        value_name = "ADDR",
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
    #[clap(value_enum, default_value_t = Trace::Stdout, help = "trace output file")]
    pub(crate) trace: Trace,
}
