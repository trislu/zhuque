use std::{
    env,
    io::{stderr, stdout},
    net::SocketAddr,
    path::PathBuf,
};

use axum::{Router, routing::get};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use tracing::{debug, subscriber};
use tracing_appender::rolling;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::cli::{Args, Trace};

mod cli;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let (non_blocking_writer, _guard) = match args.trace {
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
        .with_max_level(args.level)
        // Build the subscriber
        .finish();

    // use that subscriber to process traces emitted after this point
    subscriber::set_global_default(sub).expect("Could not set global default subscriber");

    debug!("{:?}", args);

    let config = RustlsConfig::from_pem_file(PathBuf::from(args.cert), PathBuf::from(args.key))
        .await
        .unwrap();
    let app = Router::new().route("/", get(handler));

    // run https server
    let addr = SocketAddr::from((args.addr, args.port));
    debug!("listening on {}", addr);
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[allow(dead_code)]
async fn handler() -> &'static str {
    "Hello, taikonaut!"
}
