use std::{
    env,
    io::{stderr, stdout},
    net::SocketAddr,
    sync::Arc,
};

use anyhow::{Context, Result};
use clap::Parser;
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tokio::{io::AsyncWriteExt, net::TcpListener};
use tokio_rustls::{TlsAcceptor, rustls};
use tracing::{debug, error, subscriber};
use tracing_appender::rolling;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::cli::{Args, Trace};

mod cli;
mod gmi;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse().resolve()?;

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
        // Don't display the event's target (module path)
        .with_target(false)
        // Log when entering and exiting spans
        .with_span_events(FmtSpan::NEW)
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

    debug!("with args: {:?}", args);

    let certs = CertificateDer::from_pem_file(&args.cert)
        .with_context(|| format!("failed to read cert pem file: {}", args.cert.display()))
        .map(|c| vec![c])?;
    let key = PrivateKeyDer::from_pem_file(&args.key)
        .with_context(|| format!("failed to read key pem file: {}", args.key.display()))?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    // run https server
    let addr = SocketAddr::from((args.addr, args.port));

    // display the startup badge if provided
    if let Some(badge) = &args.badge {
        println!("{badge}");
    }

    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (tcp_stream, from) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let root = args.root.clone();
        let index = args.index.clone();
        let footer = args.footer.clone();

        tokio::spawn(async move {
            match acceptor.accept(tcp_stream).await {
                Ok(mut tls_stream) => {
                    // handle the gemini request
                    let _ =
                        gmi::handle(from, &mut tls_stream, root, index, footer.as_deref()).await;
                    // flush and shutdown the stream
                    if let Err(e) = tls_stream.flush().await {
                        error!("Could not flush stream: {e}");
                    };
                    if let Err(e) = tls_stream.shutdown().await {
                        error!("Could not shut down connection: {e}");
                    };
                }
                Err(e) => {
                    error!("TLS handshake failed from {}: {}", from, e);
                }
            }
        });
    }
}
