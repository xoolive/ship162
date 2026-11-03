//! HTTP `CONNECT` proxy support for outgoing TCP connections
//!
//! Network sources open their TCP connection through this module so that they
//! work on hosts which only reach the Internet through an HTTP proxy. The proxy
//! is read from the usual environment variables, the same way as curl does:
//!
//! - `https_proxy` / `HTTPS_PROXY` for secure endpoints (`wss://`, `https://`),
//! - `http_proxy` / `HTTP_PROXY` for plain endpoints (`ws://`, `tcp://`),
//! - `all_proxy` / `ALL_PROXY` as a fallback for both,
//! - `no_proxy` / `NO_PROXY` for a comma-separated list of hosts to reach directly.
//!
//! Only `http://` proxies are supported. Credentials in the proxy URL
//! (`http://user:password@proxy:3128`) are sent with Basic authentication.
//!
//! ```no_run
//! use rs162::sources::proxy::connect;
//!
//! # async fn run() -> std::io::Result<()> {
//! // Direct connection, or a CONNECT tunnel if https_proxy is set
//! let stream = connect("stream.aisstream.io", 443, true).await?;
//! # Ok(())
//! # }
//! ```

use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::debug;

/// Default port of a proxy URL without an explicit port, as in curl
const DEFAULT_PROXY_PORT: u16 = 1080;
/// Upper bound for the proxy response header, in bytes
const MAX_RESPONSE_HEADER: usize = 16 * 1024;

/// An HTTP proxy accepting `CONNECT` requests
#[derive(Clone, PartialEq, Eq)]
pub struct HttpProxy {
    /// Proxy host name or IP address
    pub host: String,
    /// Proxy port
    pub port: u16,
    /// `user:password` credentials for Basic authentication, if any
    credentials: Option<String>,
}

impl std::fmt::Debug for HttpProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the credentials
        f.debug_struct("HttpProxy")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("credentials", &self.credentials.as_ref().map(|_| "***"))
            .finish()
    }
}

impl HttpProxy {
    /// Parse a proxy URL such as `http://proxy:3128` or `proxy:3128`
    ///
    /// Returns an error for schemes other than `http`.
    pub fn parse(value: &str) -> io::Result<Self> {
        let invalid = |reason: &str| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid proxy URL: {reason}"),
            )
        };
        let value = value.trim();
        let rest = match value.split_once("://") {
            Some(("http" | "HTTP", rest)) => rest,
            Some((scheme, _)) => {
                return Err(invalid(&format!(
                    "unsupported scheme `{scheme}`, only http:// proxies are supported"
                )))
            }
            None => value,
        };
        let authority = rest.split('/').next().unwrap_or_default();
        let (credentials, host_port) = match authority.rsplit_once('@') {
            Some((credentials, host_port)) => (Some(credentials.to_string()), host_port),
            None => (None, authority),
        };
        let (host, port) = split_host_port(host_port);
        if host.is_empty() {
            return Err(invalid("missing host"));
        }
        let port = match port {
            Some(port) => port.parse().map_err(|_| invalid("invalid port"))?,
            None => DEFAULT_PROXY_PORT,
        };
        Ok(Self {
            host: host.to_string(),
            port,
            credentials,
        })
    }
}

impl HttpProxy {
    /// `(user, password)` for Basic authentication, if the proxy URL has credentials
    #[must_use]
    pub fn credentials(&self) -> Option<(&str, &str)> {
        self.credentials
            .as_deref()
            .map(|credentials| credentials.split_once(':').unwrap_or((credentials, "")))
    }
}

/// Split `host:port`, keeping bracketed IPv6 addresses intact
fn split_host_port(value: &str) -> (&str, Option<&str>) {
    if let Some(rest) = value.strip_prefix('[') {
        if let Some((host, after)) = rest.split_once(']') {
            return (host, after.strip_prefix(':'));
        }
    }
    match value.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, Some(port)),
        _ => (value, None),
    }
}

/// Proxy settings, usually read from the environment
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxySettings {
    /// Proxy for plain endpoints
    pub http: Option<String>,
    /// Proxy for secure endpoints
    pub https: Option<String>,
    /// Hosts to reach without a proxy
    pub no_proxy: Vec<String>,
}

impl ProxySettings {
    /// Read the settings from the process environment
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Read the settings with a custom variable lookup
    #[must_use]
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let first = |names: &[&str]| {
            names
                .iter()
                .find_map(|name| lookup(name).filter(|value| !value.trim().is_empty()))
        };
        let all = first(&["all_proxy", "ALL_PROXY"]);
        Self {
            http: first(&["http_proxy", "HTTP_PROXY"]).or_else(|| all.clone()),
            https: first(&["https_proxy", "HTTPS_PROXY"]).or(all),
            no_proxy: first(&["no_proxy", "NO_PROXY"])
                .map(|value| {
                    value
                        .split(',')
                        .map(|entry| entry.trim().to_ascii_lowercase())
                        .filter(|entry| !entry.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The proxy URL to use for `host`, if any
    #[must_use]
    pub fn proxy_url_for(&self, host: &str, secure: bool) -> Option<&str> {
        let url = if secure { &self.https } else { &self.http };
        let url = url.as_deref()?;
        (!self.bypass(host)).then_some(url)
    }

    /// The proxy to use for `host`, if any
    pub fn proxy_for(&self, host: &str, secure: bool) -> io::Result<Option<HttpProxy>> {
        self.proxy_url_for(host, secure)
            .map(HttpProxy::parse)
            .transpose()
    }

    /// Whether `host` matches the `no_proxy` list
    #[must_use]
    pub fn bypass(&self, host: &str) -> bool {
        let host = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase();
        self.no_proxy.iter().any(|entry| {
            if entry == "*" {
                return true;
            }
            let (entry, _) = split_host_port(entry);
            let entry = entry.trim_start_matches("*.").trim_start_matches('.');
            host == entry || host.ends_with(&format!(".{entry}"))
        })
    }
}

/// Open a TCP connection to `host:port`, through a proxy if the environment says so
///
/// `secure` selects which proxy variable applies (`https_proxy` for TLS
/// endpoints, `http_proxy` otherwise).
pub async fn connect(host: &str, port: u16, secure: bool) -> io::Result<TcpStream> {
    connect_with(&ProxySettings::from_env(), host, port, secure).await
}

/// Open a TCP connection to `host:port` with explicit proxy settings
pub async fn connect_with(
    settings: &ProxySettings,
    host: &str,
    port: u16,
    secure: bool,
) -> io::Result<TcpStream> {
    match settings.proxy_for(host, secure)? {
        Some(proxy) => connect_via(&proxy, host, port).await,
        None => {
            TcpStream::connect((host.trim_start_matches('[').trim_end_matches(']'), port)).await
        }
    }
}

/// Open a tunnel to `host:port` through an HTTP proxy with a `CONNECT` request
pub async fn connect_via(proxy: &HttpProxy, host: &str, port: u16) -> io::Result<TcpStream> {
    debug!(
        proxy.host,
        proxy.port, host, port, "connecting through HTTP proxy"
    );
    let mut stream = TcpStream::connect((proxy.host.as_str(), proxy.port)).await?;
    let target = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(credentials) = &proxy.credentials {
        request.push_str("Proxy-Authorization: Basic ");
        request.push_str(&base64(credentials.as_bytes()));
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await?;

    // Read the response header byte by byte so that no tunnelled data is consumed
    let mut header = Vec::with_capacity(256);
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= MAX_RESPONSE_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "proxy response header too long",
            ));
        }
        let byte = stream.read_u8().await.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("proxy closed the connection during CONNECT: {error}"),
            )
        })?;
        header.push(byte);
    }
    let header = String::from_utf8_lossy(&header);
    let status_line = header.lines().next().unwrap_or_default();
    let status = status_line.split_whitespace().nth(1);
    if status != Some("200") {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("proxy refused CONNECT to {target}: {status_line}"),
        ));
    }
    Ok(stream)
}

/// Standard base64 encoding, for the `Proxy-Authorization` header
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bytes = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let value = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        for index in 0..4 {
            if index <= chunk.len() {
                let sextet = (value >> (18 - 6 * index)) & 0x3F;
                output.push(ALPHABET[sextet as usize] as char);
            } else {
                output.push('=');
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::net::TcpListener;

    fn settings(vars: &[(&str, &str)]) -> ProxySettings {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        ProxySettings::from_lookup(|name| vars.get(name).cloned())
    }

    #[test]
    fn parses_proxy_urls() {
        let proxy = HttpProxy::parse("http://localhost:8123").unwrap();
        assert_eq!((proxy.host.as_str(), proxy.port), ("localhost", 8123));
        let proxy = HttpProxy::parse("proxy.example.org").unwrap();
        assert_eq!(proxy.port, DEFAULT_PROXY_PORT);
        let proxy = HttpProxy::parse("http://user:secret@[::1]:3128/").unwrap();
        assert_eq!((proxy.host.as_str(), proxy.port), ("::1", 3128));
        assert!(!format!("{proxy:?}").contains("secret"));
        assert!(HttpProxy::parse("socks5://localhost:1080").is_err());
        assert!(HttpProxy::parse("http://:8080").is_err());
    }

    #[test]
    fn selects_proxy_by_scheme_and_no_proxy() {
        let settings = settings(&[
            ("https_proxy", "http://localhost:8123"),
            ("no_proxy", "semia,.onera.net, 127.0.0.1"),
        ]);
        assert_eq!(
            settings.proxy_url_for("stream.aisstream.io", true),
            Some("http://localhost:8123")
        );
        assert_eq!(settings.proxy_url_for("stream.aisstream.io", false), None);
        assert_eq!(settings.proxy_url_for("semia", true), None);
        assert_eq!(settings.proxy_url_for("www.onera.net", true), None);
        assert_eq!(settings.proxy_url_for("onera.net", true), None);
        assert!(settings.proxy_url_for("notonera.net", true).is_some());
        assert_eq!(settings.proxy_url_for("127.0.0.1", true), None);

        let settings = settings_all();
        assert!(settings.proxy_url_for("example.org", false).is_some());
        assert!(settings.proxy_url_for("example.org", true).is_some());
    }

    fn settings_all() -> ProxySettings {
        settings(&[("ALL_PROXY", "http://proxy:3128"), ("NO_PROXY", "")])
    }

    #[test]
    fn wildcard_no_proxy_bypasses_everything() {
        let settings = settings(&[("https_proxy", "http://proxy:3128"), ("no_proxy", "*")]);
        assert_eq!(settings.proxy_url_for("example.org", true), None);
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
    }

    /// A one-shot proxy: checks the CONNECT request, answers with `status`, then echoes
    async fn mock_proxy(status: &'static str) -> (u16, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(socket.read_u8().await.unwrap());
            }
            socket
                .write_all(format!("HTTP/1.1 {status}\r\nVia: mock\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut buffer = [0u8; 5];
            if socket.read_exact(&mut buffer).await.is_ok() {
                socket.write_all(&buffer).await.unwrap();
            }
            String::from_utf8(request).unwrap()
        });
        (port, handle)
    }

    #[tokio::test]
    async fn tunnels_through_connect() {
        let (port, handle) = mock_proxy("200 Connection established").await;
        let proxy = HttpProxy::parse(&format!("http://user:pass@127.0.0.1:{port}")).unwrap();
        let mut stream = connect_via(&proxy, "stream.aisstream.io", 443)
            .await
            .unwrap();
        stream.write_all(b"hello").await.unwrap();
        let mut echo = [0u8; 5];
        stream.read_exact(&mut echo).await.unwrap();
        assert_eq!(&echo, b"hello");
        let request = handle.await.unwrap();
        assert!(request.starts_with("CONNECT stream.aisstream.io:443 HTTP/1.1\r\n"));
        assert!(request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
    }

    #[tokio::test]
    async fn reports_refused_connect() {
        let (port, _handle) = mock_proxy("403 Forbidden").await;
        let proxy = HttpProxy::parse(&format!("127.0.0.1:{port}")).unwrap();
        let error = connect_via(&proxy, "example.org", 443).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
        assert!(error.to_string().contains("403 Forbidden"));
    }
}
