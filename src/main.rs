mod cli;

use clap::Parser;

use crate::cli::Cli;

fn main() {
    println!("Hello, taikonaut!");

    match Cli::try_parse() {
        Ok(cli) => {
            println!("{:?}", cli);
        }
        Err(e) => {
            eprintln!("{}", e);
        }
    }
}
