use std::collections::HashMap;
use std::collections::HashSet;
use std::io::BufReader;
use std::path::PathBuf;
use std::{fmt::Debug, sync::Arc};

use bytes::BytesMut;
use frame::Frame;
use frame::OpCode;
use sha1::Digest;
use stream::WsStream;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use tokio_rustls::{client::TlsStream, rustls::ClientConfig, TlsConnector};
use webpki::DNSNameRef;

pub mod errors;
pub mod frame;
pub mod proxy;
pub mod stream;
use errors::WsError;

const BUF_SIZE: usize = 4 * 1024;
const GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub struct HandshakeResponse {
    pub code: u8,
    pub reason: String,
    pub headers: HashMap<String, String>,
}

/// websocket client, use ClientBuilder to construct new client
///
/// ```rust
/// use ws_client::{frame::Frame, ClientBuilder};
/// let mut client = ClientBuilder::new("wss://privaterookie.com").proxy("socks5://proxy:proxy_port").build().unwrap();
/// client.connect().await.unwrap();
/// client.read_frame().await.unwrap();
/// ```
#[derive(Debug)]
pub struct Client {
    pub uri: http::Uri,
    mode: Mode,
    stream: Option<stream::WsStream>,
    certs: HashSet<PathBuf>,
    accept_key: String,
    pub state: ConnectionState,
    pub handshake_remaining: BytesMut,
    pub proxy: Option<proxy::Proxy>,
    pub protocols: HashSet<String>,
}

impl Client {
    /// create tcp connection & perform websocket handshake
    pub async fn connect(&mut self) -> Result<HandshakeResponse, WsError> {
        self.state = ConnectionState::Connecting;
        let host = self.uri.host().ok_or(WsError::InvalidUri(format!(
            "can not find host {}",
            self.uri
        )))?;
        let port = match self.uri.port_u16() {
            Some(port) => port,
            None => self.mode.default_port(),
        };
        let stream = match &self.proxy {
            Some(proxy_conf) => proxy_conf.connect((host, port)).await?,
            None => TcpStream::connect((host, port)).await.map_err(|e| {
                WsError::ConnectionFailed(format!(
                    "failed to create tcp connection {}",
                    e.to_string()
                ))
            })?,
        };
        log::debug!("tcp connection established");
        let mut stream = match self.mode {
            Mode::WS => WsStream::Plain(stream),
            Mode::WSS => {
                let tls_stream = wrap_tls(stream, host, &self.certs).await?;
                WsStream::Tls(tls_stream)
            }
        };
        self.state = ConnectionState::HandShaking;
        let (resp, remain_bytes) = self.perform_handshake(&mut stream).await?;
        self.handshake_remaining = remain_bytes;
        self.stream = Some(stream);
        Ok(resp)
    }

    async fn perform_handshake(
        &mut self,
        stream: &mut WsStream,
    ) -> Result<(HandshakeResponse, BytesMut), WsError> {
        let protocols = self
            .protocols
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<String>>();
        let key = gen_key();
        self.accept_key = cal_accept_key(&key);

        let req = http::Request::builder()
            .uri(&self.uri)
            .header(
                "Host",
                format!(
                    "{}:{}",
                    self.uri.host().unwrap_or_default(),
                    self.uri
                        .port_u16()
                        .unwrap_or_else(|| self.mode.default_port())
                ),
            )
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-Websocket-Key", &key)
            .header("Sec-WebSocket-Protocol", protocols.join(" "))
            .header("Sec-WebSocket-Version", "13")
            .body(())
            .unwrap();
        let headers = req
            .headers()
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or_default()))
            .collect::<Vec<String>>()
            .join("\r\n");
        let method = http::Method::GET;
        let req_str = format!(
            "{method} {path} {version:?}\r\n{headers}\r\n\r\n",
            method = method,
            path = self
                .uri
                .path_and_query()
                .map(|full_path| full_path.to_string())
                .unwrap_or_default(),
            version = http::Version::HTTP_11,
            headers = headers
        );
        stream
            .write_all(req_str.as_bytes())
            .await
            .map_err(|e| WsError::IOError(e.to_string()))?;
        let mut read_bytes = BytesMut::with_capacity(1024);
        let mut buf: [u8; 1024] = [0; 1024];
        loop {
            let num = stream
                .read(&mut buf)
                .await
                .map_err(|e| WsError::IOError(e.to_string()))?;
            read_bytes.extend_from_slice(&buf[..num]);
            let header_complete = read_bytes
                .windows(4)
                .any(|slice| slice == [b'\r', b'\n', b'\r', b'\n']);
            if header_complete || num == 0 {
                break;
            }
        }
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut resp = httparse::Response::new(&mut headers);
        let parse_status = resp
            .parse(&read_bytes)
            .map_err(|_| WsError::HandShakeFailed(format!("invalid response")))?;
        let header_len = match parse_status {
            httparse::Status::Complete(len) => Ok(len),
            httparse::Status::Partial => Err(WsError::HandShakeFailed(format!(
                "incomplete handshake response"
            ))),
        }?;
        if resp.code.unwrap_or_default() != 101 {
            return Err(WsError::HandShakeFailed(format!(
                "expect 101 response, got {:?} {:?}",
                resp.code, resp.reason
            )));
        }
        for header in resp.headers.iter() {
            if header.name.to_lowercase() == "sec-websocket-accept" {
                if header.value != self.accept_key.as_bytes() {
                    return Err(WsError::HandShakeFailed(format!(
                        "mismatch key, expect {:?}, got {:?}",
                        self.accept_key.as_bytes(),
                        header.value
                    )));
                }
            }
        }
        let mut handshake_resp = HandshakeResponse {
            code: 101,
            reason: resp.reason.map(|r| r.to_string()).unwrap_or_default(),
            headers: HashMap::new(),
        };
        resp.headers.iter().for_each(|header| {
            handshake_resp.headers.insert(
                header.name.to_string(),
                String::from_utf8_lossy(header.value).to_string(),
            );
        });
        log::debug!("protocol handshake complete");
        Ok((handshake_resp, BytesMut::from(&read_bytes[header_len..])))
    }

    pub async fn read_frame(&mut self) -> Result<Frame, WsError> {
        let stream = self.stream.as_mut().unwrap();
        let mut source = BytesMut::with_capacity(BUF_SIZE / 4);
        let mut leading_bytes = [0u8; 2];
        let mut chained_stream = self.handshake_remaining.chain(stream);
        chained_stream
            .read_exact(&mut leading_bytes)
            .await
            .map_err(|e| WsError::IOError(e.to_string()))?;
        source.extend_from_slice(&leading_bytes);
        let leading_len = (leading_bytes[1] << 1) >> 1;
        let payload_len: usize = match leading_len {
            0..=125 => Ok(leading_len as usize),
            126 => {
                let mut len_bytes = [0u8; 2];
                chained_stream
                    .read_exact(&mut len_bytes)
                    .await
                    .map_err(|e| WsError::IOError(e.to_string()))?;
                source.extend_from_slice(&len_bytes);
                Ok(u16::from_be_bytes(len_bytes) as usize)
            }
            127 => {
                let mut len_bytes = [0u8; 8];
                chained_stream
                    .read_exact(&mut len_bytes)
                    .await
                    .map_err(|e| WsError::IOError(e.to_string()))?;
                source.extend_from_slice(&len_bytes);
                Ok(u64::from_be_bytes(len_bytes) as usize)
            }
            _ => Err(WsError::ProtocolError(format!(
                "invalid leading len {}",
                leading_len
            ))),
        }?;
        let start_idx = source.len();
        let new_size = start_idx + payload_len;
        source.resize(new_size, 0);
        chained_stream
            .read_exact(&mut source[start_idx..])
            .await
            .map_err(|e| WsError::IOError(e.to_string()))?;
        let remaining_idx = new_size.min(self.handshake_remaining.len());
        self.handshake_remaining = BytesMut::from(&self.handshake_remaining[remaining_idx..]);
        let frame = Frame::from_bytes(source).map_err(|e| WsError::ProtocolError(e.to_string()))?;
        Ok(frame)
    }

    pub async fn write_frame(&mut self, frame: Frame) -> Result<(), WsError> {
        let stream = self.stream.as_mut().unwrap();
        stream
            .write_all(frame.as_bytes())
            .await
            .map_err(|e| WsError::IOError(e.to_string()))?;
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), WsError> {
        self.state = ConnectionState::Closing;
        let mut close = Frame::new_with_opcode(OpCode::Close);
        close.set_payload(&1000u16.to_be_bytes());
        self.write_frame(close).await
    }
}

pub struct ClientBuilder {
    uri: String,
    proxy_uri: Option<String>,
    protocols: HashSet<String>,
    certs: HashSet<PathBuf>,
}

impl ClientBuilder {
    pub fn new(uri: &str) -> Self {
        Self {
            uri: uri.to_string(),
            proxy_uri: None,
            protocols: HashSet::new(),
            certs: HashSet::new(),
        }
    }

    /// config  proxy
    pub fn proxy(self, uri: &str) -> Self {
        Self {
            proxy_uri: Some(uri.to_string()),
            ..self
        }
    }

    /// add protocols
    pub fn protocol(mut self, protocol: String) -> Self {
        self.protocols.insert(protocol);
        self
    }

    /// set protocols in handshake http header
    ///
    /// **NOTE** it will clear protocols set by `protocol` method
    pub fn protocols(self, protocols: HashSet<String>) -> Self {
        Self { protocols, ..self }
    }

    pub fn cert(mut self, cert: PathBuf) -> Self {
        self.certs.insert(cert);
        self
    }

    // set ssl certs in wss connection
    ///
    /// **NOTE** it will clear certs set by `cert` method
    pub fn certs(self, certs: HashSet<PathBuf>) -> Self {
        Self { certs, ..self }
    }

    pub fn build(&self) -> Result<Client, WsError> {
        let Self {
            uri,
            proxy_uri,
            protocols,
            certs,
        } = self;
        let uri = uri
            .parse::<http::Uri>()
            .map_err(|e| WsError::InvalidUri(format!("{} {}", uri, e.to_string())))?;
        let mode = if let Some(schema) = uri.scheme_str() {
            match schema.to_ascii_lowercase().as_str() {
                "ws" => Ok(Mode::WS),
                "wss" => Ok(Mode::WSS),
                _ => Err(WsError::InvalidUri(format!("invalid schema {}", schema))),
            }
        } else {
            Err(WsError::InvalidUri(format!("missing ws or wss schema")))
        }?;
        if mode == Mode::WS && !certs.is_empty() {
            log::warn!("setting tls cert has no effect on insecure ws")
        }
        let ws_proxy: Option<proxy::Proxy> = match proxy_uri {
            Some(uri) => Some(uri.parse()?),
            None => None,
        };

        Ok(Client {
            uri,
            mode,
            stream: None,
            state: ConnectionState::Created,
            proxy: ws_proxy,
            accept_key: String::new(),
            handshake_remaining: BytesMut::with_capacity(0),
            certs: certs.iter().map(|p| p.into()).collect(),
            protocols: protocols.clone(),
        })
    }
}

/// websocket connection state
#[derive(Debug, Clone)]
pub enum ConnectionState {
    /// init state
    Created,
    /// tcp & tls connection creating state
    Connecting,
    /// perform websocket handshake
    HandShaking,
    /// websocket connection has been successfully established
    Running,
    /// client or peer has send "close frame"
    Closing,
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    WS,
    WSS,
}

impl Mode {
    fn default_port(&self) -> u16 {
        match self {
            Mode::WS => 80,
            Mode::WSS => 443,
        }
    }
}

async fn wrap_tls(
    stream: TcpStream,
    host: &str,
    certs: &HashSet<PathBuf>,
) -> Result<TlsStream<TcpStream>, WsError> {
    let mut config = ClientConfig::new();
    for cert_path in certs {
        let mut pem = std::fs::File::open(cert_path).map_err(|_| {
            WsError::CertFileNotFound(cert_path.to_str().unwrap_or_default().to_string())
        })?;
        let mut cert = BufReader::new(&mut pem);
        config.root_store.add_pem_file(&mut cert).map_err(|_| {
            WsError::CertFileNotFound(cert_path.to_str().unwrap_or_default().to_string())
        })?;
    }
    config
        .root_store
        .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);
    let domain =
        DNSNameRef::try_from_ascii_str(host).map_err(|e| WsError::TlsDnsFailed(e.to_string()))?;
    let connector = TlsConnector::from(Arc::new(config));
    let tls_stream = connector
        .connect(domain, stream)
        .await
        .map_err(|e| WsError::ConnectionFailed(e.to_string()))?;
    log::debug!("tls connection established");
    Ok(tls_stream)
}

fn gen_key() -> String {
    let r: [u8; 16] = rand::random();
    base64::encode(&r)
}

fn cal_accept_key(source: &str) -> String {
    let mut sha1 = sha1::Sha1::default();
    sha1.update(source.as_bytes());
    sha1.update(GUID);
    base64::encode(&sha1.finalize())
}
