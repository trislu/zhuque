use std::{
    env,
    io::{stderr, stdout},
};

use clap::Parser;
use tracing::{debug, info, subscriber};
use tracing_appender::rolling;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::cli::{Cli, Trace};

mod cli;

fn main() {
    let cli = Cli::parse();

    let (non_blocking_writer, _guard) = match cli.trace {
        Trace::Stdout => tracing_appender::non_blocking(stdout()),
        Trace::Stderr => tracing_appender::non_blocking(stderr()),
        Trace::Tmp => {
            //let location = temp_location.unwrap_or_default();
            let temp_dir = env::temp_dir().join("zhuque-gemini");
            let file_appender = rolling::hourly(temp_dir, "zq.gmi");
            tracing_appender::non_blocking(file_appender)
        }
    };

    let sub = tracing_subscriber::fmt()
        // Use a more compact, abbreviated log format
        .compact()
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the thread ID an event was recorded on
        .with_thread_ids(true)
        // Don't display the event's target (module path)
        .with_target(false)
        // Log when entering and exiting spans
        .with_span_events(FmtSpan::ENTER)
        // log to a file
        .with_writer(non_blocking_writer)
        // Disabled ANSI color codes for better compatibility with some terminals
        .with_ansi(false)
        // TODO: log level control
        .with_max_level(cli.level)
        // Build the subscriber
        .finish();

    // use that subscriber to process traces emitted after this point
    subscriber::set_global_default(sub).expect("Could not set global default subscriber");

    debug!("{:?}", cli);

    info!("Hello, taikonaut!");
}
