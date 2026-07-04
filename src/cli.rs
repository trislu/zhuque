use std::{net::IpAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use tracing::level_filters::LevelFilter;

#[derive(Clone, Debug, ValueEnum, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Trace {
    Stdout,
    Stderr,
    Tmp,
}

#[derive(Clone, Copy, Debug, ValueEnum, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    addr: Option<IpAddr>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    cert: Option<PathBuf>,
    #[serde(default)]
    key: Option<PathBuf>,
    #[serde(default)]
    trace: Option<Trace>,
    #[serde(default)]
    level: Option<Level>,
    #[serde(default)]
    root: Option<PathBuf>,
    #[serde(default)]
    index: Option<PathBuf>,
    #[serde(default)]
    badge: Option<String>,
    #[serde(default)]
    footer: Option<String>,
}

impl ConfigFile {
    fn from_path(path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("failed to parse config file: {}", path.display()))
    }
}

#[derive(Debug, Clone, Parser)]
#[command(author, about = "zhuque", long_about = None)]
pub(crate) struct Args {
    #[arg(
        long = "config",
        value_name = "FILE",
        help = "path to a TOML config file (mutually exclusive with CLI options)"
    )]
    pub(crate) config: Option<PathBuf>,
    #[arg(
        short = 'a',
        long = "addr",
        value_name = "ADDR",
        help = "server address"
    )]
    pub(crate) addr: Option<IpAddr>,
    #[arg(short = 'p', long = "port", value_name = "PORT", help = "server port")]
    pub(crate) port: Option<u16>,
    #[arg(
        short = 'c',
        long = "cert",
        value_name = "FILE",
        help = "cert pem file path"
    )]
    pub(crate) cert: Option<PathBuf>,
    #[arg(
        short = 'k',
        long = "key",
        value_name = "FILE",
        help = "key pem file path"
    )]
    pub(crate) key: Option<PathBuf>,
    #[clap(value_enum, short = 't', long = "trace", help = "trace output")]
    pub(crate) trace: Option<Trace>,
    #[clap(value_enum, short = 'l', long = "level", help = "trace output level")]
    pub(crate) level: Option<Level>,
    #[arg(
        short = 'r',
        long = "root",
        value_name = "PATH",
        help = "capsule root path"
    )]
    pub(crate) root: Option<PathBuf>,
    #[arg(
        short = 'i',
        long = "index",
        value_name = "FILE",
        help = "capsule index page"
    )]
    pub(crate) index: Option<PathBuf>,
    #[arg(
        short = 'b',
        long = "badge",
        value_name = "TEXT",
        help = "message shown when the server starts"
    )]
    pub(crate) badge: Option<String>,
    #[arg(
        short = 'f',
        long = "footer",
        value_name = "TEXT",
        help = "footer text appended to text/gemini content responses"
    )]
    pub(crate) footer: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedArgs {
    pub(crate) addr: IpAddr,
    pub(crate) port: u16,
    pub(crate) cert: PathBuf,
    pub(crate) key: PathBuf,
    pub(crate) trace: Trace,
    pub(crate) level: Level,
    pub(crate) root: PathBuf,
    pub(crate) index: PathBuf,
    pub(crate) badge: Option<String>,
    pub(crate) footer: Option<String>,
}

impl Args {
    pub(crate) fn resolve(self) -> Result<ResolvedArgs> {
        if let Some(path) = self.config {
            if self.addr.is_some()
                || self.port.is_some()
                || self.cert.is_some()
                || self.key.is_some()
                || self.trace.is_some()
                || self.level.is_some()
                || self.root.is_some()
                || self.index.is_some()
            {
                bail!("--config cannot be used together with CLI options");
            }

            let config = ConfigFile::from_path(&path)?;
            return Ok(ResolvedArgs {
                addr: config.addr.unwrap_or("127.0.0.1".parse().unwrap()),
                port: config.port.unwrap_or(1965),
                cert: config.cert.context("missing cert in config")?,
                key: config.key.context("missing key in config")?,
                trace: config.trace.unwrap_or(Trace::Stderr),
                level: config.level.unwrap_or(Level::Info),
                root: config.root.context("missing root in config")?,
                index: config.index.unwrap_or_else(|| PathBuf::from("index.gmi")),
                badge: config.badge,
                footer: config.footer,
            });
        }

        Ok(ResolvedArgs {
            addr: self.addr.unwrap_or("127.0.0.1".parse().unwrap()),
            port: self.port.unwrap_or(1965),
            cert: self.cert.context("missing cert")?,
            key: self.key.context("missing key")?,
            root: self.root.context("missing root")?,
            trace: self.trace.unwrap_or(Trace::Stderr),
            level: self.level.unwrap_or(Level::Info),
            index: self.index.unwrap_or_else(|| PathBuf::from("index.gmi")),
            badge: self.badge,
            footer: self.footer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zhuque-{name}-{nanos}.toml"))
    }

    #[test]
    fn config_file_can_be_loaded_from_toml() {
        let path = temp_path("config");
        fs::write(
            &path,
            r#"
addr = "127.0.0.1"
port = 1965
cert = "cert.pem"
key = "key.pem"
root = "gemcap"
trace = "stderr"
level = "info"
badge = "startup badge"
footer = "server footer"
"#,
        )
        .unwrap();

        let cfg = ConfigFile::from_path(&path).unwrap();
        assert_eq!(cfg.addr, Some("127.0.0.1".parse().unwrap()));
        assert_eq!(cfg.port, Some(1965));
        assert_eq!(cfg.cert.as_deref(), Some(Path::new("cert.pem")));
        assert_eq!(cfg.key.as_deref(), Some(Path::new("key.pem")));
        assert_eq!(cfg.root.as_deref(), Some(Path::new("gemcap")));
        assert_eq!(cfg.trace, Some(Trace::Stderr));
        assert_eq!(cfg.level, Some(Level::Info));
        assert_eq!(cfg.badge.as_deref(), Some("startup badge"));
        assert_eq!(cfg.footer.as_deref(), Some("server footer"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn config_file_rejects_cli_options() {
        let path = temp_path("mixed");
        fs::write(
            &path,
            "root = 'gemcap'\ncert = 'cert.pem'\nkey = 'key.pem'\n",
        )
        .unwrap();

        let args = Args {
            config: Some(path.clone()),
            addr: Some("127.0.0.1".parse().unwrap()),
            port: None,
            cert: None,
            key: None,
            trace: None,
            level: None,
            root: None,
            index: None,
            badge: None,
            footer: None,
        };

        let err = args.resolve().unwrap_err();
        assert!(err.to_string().contains("--config cannot be used together"));

        let _ = fs::remove_file(path);
    }
}
