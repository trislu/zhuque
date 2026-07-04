use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, info, instrument};
use url::Url;

macro_rules! gemini_scheme {
    () => {
        "gemini"
    };
}

const GEMINI_SCHEME: &str = gemini_scheme!();
const GEMINI_MIME: &str = concat!("text/", gemini_scheme!());

const REQUEST_URI_MAX_BYTES: usize = 1024;
const REQUEST_TAIL_CRLF: &str = "\r\n";
const REQUEST_MAX_BYTES: usize = REQUEST_URI_MAX_BYTES + REQUEST_TAIL_CRLF.len();

#[derive(Debug, Clone)]
struct WithErrorMessage<T> {
    code: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T> WithErrorMessage<T> {
    const fn new(code: usize) -> Self {
        Self {
            code,
            _marker: std::marker::PhantomData,
        }
    }

    fn build(self, msg: &str) -> String {
        if msg.is_empty() {
            format!("{}\r\n", self.code)
        } else {
            format!("{} {msg}\r\n", self.code)
        }
    }
}

struct TemporaryFailure;
type Tempfail = WithErrorMessage<TemporaryFailure>;

#[allow(dead_code)]
impl Tempfail {
    const UNSPECIFIED: Self = Self::new(40);
    const SERVER_UNAVAILABLE: Self = Self::new(41);
    const CGI_ERROR: Self = Self::new(42);
    const PROXY_ERROR: Self = Self::new(43);
    const SLOW_DOWN: Self = Self::new(44);
}

struct PermanentFailure;
type Permfail = WithErrorMessage<PermanentFailure>;
#[allow(dead_code)]
impl Permfail {
    const GENERAL: Self = Self::new(50);
    const NOT_FOUND: Self = Self::new(51);
    const GONE: Self = Self::new(52);
    const PROXY_REQUEST_REFUSED: Self = Self::new(53);
    const BAD_REQUEST: Self = Self::new(59);
}

struct ClientCertificates;
type Auth = WithErrorMessage<ClientCertificates>;
#[allow(dead_code)]
impl Auth {
    const CLIENT_CERTIFICATES_REQUIRED: Self = Self::new(60);
    const CERTIFICATE_NOT_AUTHORIZED: Self = Self::new(61);
    const CERTIFICATE_NOT_VALID: Self = Self::new(62);
}

fn is_path_traversal(path: &str) -> bool {
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
    let pathbuf = PathBuf::from(decoded.as_ref());
    pathbuf
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

fn parse_request_url(request: &str) -> Result<Url, String> {
    let Some(term_pos) = request.find(REQUEST_TAIL_CRLF) else {
        return Err("missing CRLF termination".to_string());
    };

    if term_pos + REQUEST_TAIL_CRLF.len() < request.len() {
        return Err("tailing data after CRLF termination".to_string());
    }

    let request_url = &request[..term_pos];
    if is_path_traversal(request_url) {
        return Err("request URL contains path traversal".to_string());
    }

    match Url::parse(request_url) {
        Ok(url) => match url.scheme() {
            GEMINI_SCHEME => Ok(url),
            others => Err(format!("invalid URL scheme: {}", others)),
        },
        Err(e) => Err(format!("invalid request URL: {e}")),
    }
}

fn parse_request_bytes(buffer: &[u8], size: usize) -> Result<Url, String> {
    match size {
        0 => Err(Permfail::BAD_REQUEST.build("empty request")),
        n if n > REQUEST_MAX_BYTES => {
            Err(Permfail::BAD_REQUEST.build("request exceeds maximum length"))
        }
        n => str::from_utf8(&buffer[..n]).map_or_else(
            |e| Err(Permfail::BAD_REQUEST.build(&format!("not utf-8 request: {e}"))),
            |request| parse_request_url(request).map_err(|e| Permfail::BAD_REQUEST.build(&e)),
        ),
    }
}

async fn parse_request(stream: &mut TlsStream<TcpStream>) -> Result<Url, String> {
    let mut buffer = [0u8; REQUEST_MAX_BYTES + 1];
    match stream.read(&mut buffer).await {
        Ok(n) => parse_request_bytes(&buffer, n),
        Err(e) => Err(Tempfail::UNSPECIFIED.build(&format!("failed to read request: {e}"))),
    }
}

#[instrument(level = "info", skip(url))]
async fn get_realpath(root: &PathBuf, index: &PathBuf, url: &Url) -> Result<PathBuf, String> {
    let realpath = match url.path().is_empty() {
        true => root.join(index.clone()),
        false => root.join(url.path().trim_start_matches('/')),
    };
    let realpath = match realpath.is_dir() {
        true => realpath.join("index.gmi"),
        false => realpath.to_path_buf(),
    };
    match realpath.canonicalize() {
        Ok(p) => {
            if p.starts_with(root) {
                Ok(p)
            } else {
                Err(Permfail::GENERAL.build("path traversal outside root"))
            }
        }
        Err(_) => Err(Permfail::NOT_FOUND.build("target file not found")),
    }
}

#[instrument(level = "info", skip(stream))]
pub(crate) async fn handle(
    from: SocketAddr,
    stream: &mut TlsStream<TcpStream>,
    root: PathBuf,
    index: PathBuf,
) -> anyhow::Result<()> {
    // step1: parse request url from stream
    let url = match parse_request(stream).await {
        Ok(url) => {
            debug!("request URL: {url}");
            url
        }
        Err(e) => {
            error!("invalid request: {e}");
            if let Err(e) = stream.write_all(e.as_bytes()).await {
                error!("failed to write response: {e}");
            }
            return Ok(());
        }
    };

    // TODO: handle INPUT with url.query()
    let realpath = match get_realpath(&root, &index, &url).await {
        Ok(p) => p,
        Err(e) => {
            error!("failed to get realpath: {e}");
            if let Err(e) = stream.write_all(e.as_bytes()).await {
                error!("failed to write response: {e}");
            }
            return Ok(());
        }
    };

    let mime = match realpath.extension().and_then(|ext| ext.to_str()) {
        Some("gmi") => GEMINI_MIME,
        _ => tree_magic_mini::from_filepath(realpath.as_ref()).unwrap_or(GEMINI_MIME),
    };

    info!("from {from} request {url} => realpath: {realpath:?}, mime: {mime}");

    let response_header = format!("20 {mime}\r\n");
    stream.write_all(response_header.as_bytes()).await?;

    let mut file = fs::File::open(&realpath).await?;
    tokio::io::copy(&mut file, stream).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_error_message_build_empty_and_nonempty() {
        let w = WithErrorMessage::<()>::new(42);
        assert_eq!(w.build(""), "42\r\n");

        let w2 = WithErrorMessage::<()>::new(99);
        assert_eq!(w2.build("oops"), "99 oops\r\n");
    }

    #[test]
    fn parse_request_url_success() {
        let ok = parse_request_url("gemini://example.com/path\r\n").expect("should parse");
        assert_eq!(ok.scheme(), GEMINI_SCHEME);
        assert_eq!(ok.host_str(), Some("example.com"));
        assert_eq!(ok.path(), "/path");
    }

    #[test]
    fn parse_request_url_rejects_invalid_requests() {
        let e = parse_request_url("gemini://example.com/path").unwrap_err();
        assert_eq!(e, "missing CRLF termination");

        let e = parse_request_url("gemini://example.com/path\r\nextra").unwrap_err();
        assert_eq!(e, "tailing data after CRLF termination");

        let e = parse_request_url("gemini://example.com/../etc\r\n").unwrap_err();
        assert_eq!(e, "request URL contains path traversal");

        let e = parse_request_url("gemini://example.com/foo/../etc\r\n").unwrap_err();
        assert_eq!(e, "request URL contains path traversal");

        let e =
            parse_request_url("gemini://example.com/foo/%2e%2e/%2e%2e/etc/passwd\r\n").unwrap_err();
        assert_eq!(e, "request URL contains path traversal");

        let e = parse_request_url("http://example.com/\r\n").unwrap_err();
        assert_eq!(e, "invalid URL scheme: http");

        let e = parse_request_url("not a URL\r\n").unwrap_err();
        assert!(e.starts_with("invalid request URL:"));
    }

    #[test]
    fn parse_request_bytes_rejects_bad_requests() {
        let err = parse_request_bytes(&[], 0).unwrap_err();
        assert_eq!(err, Permfail::BAD_REQUEST.build("empty request"));

        let too_long = [1u8; REQUEST_MAX_BYTES + 1];
        let err = parse_request_bytes(&too_long, too_long.len()).unwrap_err();
        assert_eq!(
            err,
            Permfail::BAD_REQUEST.build("request exceeds maximum length")
        );

        let buf = [0xffu8];
        let err = parse_request_bytes(&buf, 1).unwrap_err();
        assert!(err.contains("not utf-8 request:"));

        let err = parse_request_bytes(b"not a URL\r\n", 11).unwrap_err();
        assert!(err.starts_with("59 invalid request URL:"));
    }

    #[test]
    fn parse_request_bytes_accepts_valid_and_exactly_max_length_requests() {
        let req = b"gemini://host/hello\r\n";
        let url = parse_request_bytes(req, req.len()).expect("valid URL");
        assert_eq!(url.scheme(), "gemini");
        assert_eq!(url.host_str(), Some("host"));
        assert_eq!(url.path(), "/hello");

        let mut long_req = b"gemini://host/".to_vec();
        long_req.extend(vec![b'a'; REQUEST_URI_MAX_BYTES - long_req.len()]);
        long_req.extend(b"\r\n");
        assert_eq!(long_req.len(), REQUEST_MAX_BYTES);
        let url = parse_request_bytes(&long_req, long_req.len()).expect("valid URL");
        assert_eq!(url.scheme(), "gemini");
    }
}
