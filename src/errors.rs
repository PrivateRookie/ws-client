use thiserror::Error;

use crate::{frame::OpCode, ConnectionState};

/// errors during handshake, read/write frame
#[derive(Debug, Error)]
pub enum WsError {
    #[error("invalid uri `{0}`")]
    InvalidUri(String),
    #[error("unsupported proxy, expect socks5 or http, got {0}")]
    UnsupportedProxy(String),
    #[error("invalid proxy {0}")]
    InvalidProxy(String),
    #[error("cert {0} not found")]
    CertFileNotFound(String),
    #[error("load cert {0} failed")]
    LoadCertFailed(String),
    #[error("connection failed `{0}`")]
    ConnectionFailed(String),
    #[error("tls dns lookup failed `{0}`")]
    TlsDnsFailed(String),
    #[error("io error {0:?}")]
    IOError(Box<dyn std::error::Error + Send + Sync>),
    #[error("{0}")]
    HandShakeFailed(String),
    #[error("{error:?}")]
    ProtocolError {
        close_code: u16,
        error: ProtocolError,
    },
    #[error("proxy error `{0}`")]
    ProxyError(String),
    #[error("io on invalid connection state {0:?}")]
    InvalidConnState(ConnectionState),
    #[error("unsupported frame {0:?}")]
    UnsupportedFrame(OpCode),
}

impl From<std::io::Error> for WsError {
    fn from(e: std::io::Error) -> Self {
        WsError::IOError(Box::new(e))
    }
}

/// errors during decode frame from bytes
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("insufficient data len {0}")]
    InsufficientLen(usize),
    #[error("invalid leading bits {0:b}")]
    InvalidLeadingBits(u8),
    #[error("invalid opcode {0}")]
    InvalidOpcode(u8),
    #[error("invalid leading payload len {0}")]
    InvalidLeadingLen(u8),
    #[error("mismatch data len, expect {0}, got {1}")]
    MisMatchDataLen(usize, usize),
    #[error("missing init fragmented frame")]
    MissInitialFragmentedFrame,
    #[error("not continue frame after init fragmented frame")]
    NotContinueFrameAfterFragmented,
    #[error("fragmented control frame ")]
    FragmentedControlFrame,
    #[error("control frame is too big {0}")]
    ControlFrameTooBig(usize),
    #[error("invalid close frame payload len, expect 0, >= 2")]
    InvalidCloseFramePayload,
    #[error("invalid utf-8 text")]
    InvalidUtf8,
    #[error("invalid close code {0}")]
    InvalidCloseCode(u16),
    #[error("payload too large, max payload size {0}")]
    PayloadTooLarge(usize),
}
