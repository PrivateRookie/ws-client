//! rust websocket toolkit

#![warn(missing_docs)]
#![cfg_attr(docrs, feature(doc_auto_cfg))]

use std::collections::HashMap;

use bytes::{Bytes, BytesMut};
use errors::WsError;
use frame::{BorrowedFrame, OpCode, ReadFrame};

pub use http;

/// websocket error definitions
pub mod errors;
/// websocket transport unit
pub mod frame;
/// build connection & read/write frame utils
pub mod protocol;

/// stream definition
pub mod stream;

/// frame codec impl
pub mod codec;

fn check_uri(uri: &http::Uri) -> Result<(), WsError> {
    if let Some(scheme) = uri.scheme_str() {
        match scheme.to_lowercase().as_str() {
            "ws" | "wss" => Ok(()),
            s => Err(WsError::InvalidUri(format!("unknown scheme {s}"))),
        }
    } else {
        Err(WsError::InvalidUri("missing scheme".into()))
    }
}

/// helper builder to construct websocket client
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    protocols: Vec<String>,
    extensions: Vec<String>,
    version: u8,
    headers: HashMap<String, String>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            protocols: vec![],
            extensions: vec![],
            headers: HashMap::new(),
            version: 13,
        }
    }
}

impl ClientBuilder {
    /// create builder with websocket url
    pub fn new() -> Self {
        Default::default()
    }

    /// add protocols
    pub fn protocol(mut self, protocol: String) -> Self {
        self.protocols.push(protocol);
        self
    }

    /// set extension in handshake http header
    ///
    /// **NOTE** it will clear protocols set by `protocol` method
    pub fn protocols(self, protocols: Vec<String>) -> Self {
        Self { protocols, ..self }
    }

    /// add protocols
    pub fn extension(mut self, extension: String) -> Self {
        self.extensions.push(extension);
        self
    }

    /// set extension in handshake http header
    ///
    /// **NOTE** it will clear protocols set by `protocol` method
    pub fn extensions(self, extensions: Vec<String>) -> Self {
        Self { extensions, ..self }
    }

    /// set websocket version
    pub fn version(self, version: u8) -> Self {
        Self { version, ..self }
    }

    /// add initial request header
    pub fn header<K: ToString, V: ToString>(mut self, name: K, value: V) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// set initial request headers
    ///
    /// **NOTE** it will clear header set by previous `header` method
    pub fn headers(self, headers: HashMap<String, String>) -> Self {
        Self { headers, ..self }
    }
}

#[cfg(feature = "sync")]
mod blocking {
    use std::io::{Read, Write};

    use crate::{
        check_uri,
        errors::WsError,
        protocol::{handle_handshake, req_handshake},
        stream::WsStream,
        ClientBuilder, ServerBuilder,
    };

    impl ClientBuilder {
        /// perform protocol handshake & check server response
        pub fn connect<C, F, S>(
            &self,
            uri: http::Uri,
            mut stream: S,
            mut check_fn: F,
        ) -> Result<C, WsError>
        where
            S: Read + Write,
            F: FnMut(String, http::Response<()>, WsStream<S>) -> Result<C, WsError>,
        {
            check_uri(&uri)?;
            let (key, resp) = req_handshake(
                &mut stream,
                &uri,
                &self.protocols,
                &self.extensions,
                self.version,
                self.headers.clone(),
            )?;
            check_fn(key, resp, WsStream::new(stream))
        }
    }

    impl ServerBuilder {
        /// wait for protocol handshake from client
        /// checking handshake & construct server
        pub fn accept<F1, F2, T, C, S>(
            stream: S,
            mut handshake_handler: F1,
            mut codec_factory: F2,
        ) -> Result<C, WsError>
        where
            S: Read + Write,
            F1: FnMut(http::Request<()>) -> Result<(http::Request<()>, http::Response<T>), WsError>,
            F2: FnMut(http::Request<()>, WsStream<S>) -> Result<C, WsError>,
            T: ToString + std::fmt::Debug,
        {
            let mut stream = WsStream::new(stream);
            let req = handle_handshake(&mut stream)?;
            let (req, resp) = handshake_handler(req)?;
            let mut resp_lines = vec![format!("{:?} {}", resp.version(), resp.status())];
            resp.headers().iter().for_each(|(k, v)| {
                resp_lines.push(format!("{}: {}", k, v.to_str().unwrap_or_default()))
            });
            resp_lines.push("\r\n".to_string());
            stream.write_all(resp_lines.join("\r\n").as_bytes())?;
            tracing::debug!("{:?}", &resp);
            if resp.status() != http::StatusCode::SWITCHING_PROTOCOLS {
                return Err(WsError::HandShakeFailed(resp.body().to_string()));
            }
            codec_factory(req, stream)
        }
    }
}

#[cfg(feature = "async")]
mod non_blocking {
    use std::fmt::Debug;

    use bytes::BytesMut;
    use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

    use crate::{
        check_uri,
        errors::WsError,
        protocol::{async_handle_handshake, async_req_handshake},
        stream::WsAsyncStream,
        ServerBuilder,
    };

    use super::ClientBuilder;

    impl ClientBuilder {
        /// async version of connect
        ///
        /// perform protocol handshake & check server response
        pub async fn async_connect<C, F, S>(
            &self,
            uri: http::Uri,
            mut stream: S,
            mut check_fn: F,
        ) -> Result<C, WsError>
        where
            S: AsyncRead + AsyncWrite + Unpin,
            F: FnMut(String, http::Response<()>, BytesMut, WsAsyncStream<S>) -> Result<C, WsError>,
        {
            check_uri(&uri)?;
            let (key, resp, remain) = async_req_handshake(
                &mut stream,
                &uri,
                &self.protocols,
                &self.extensions,
                self.version,
                self.headers.clone(),
            )
            .await?;
            check_fn(key, resp, remain, WsAsyncStream::new(stream))
        }
    }

    impl ServerBuilder {
        /// async version
        ///
        /// wait for protocol handshake from client
        /// checking handshake & construct server
        pub async fn async_accept<F1, F2, T, C, S>(
            stream: S,
            mut handshake_handler: F1,
            mut codec_factory: F2,
        ) -> Result<C, WsError>
        where
            S: AsyncRead + AsyncWrite + Unpin,
            F1: FnMut(http::Request<()>) -> Result<(http::Request<()>, http::Response<T>), WsError>,
            F2: FnMut(http::Request<()>, BytesMut, WsAsyncStream<S>) -> Result<C, WsError>,
            T: ToString + Debug,
        {
            let mut stream = WsAsyncStream::new(stream);
            let (req, remain) = async_handle_handshake(&mut stream).await?;
            let (req, resp) = handshake_handler(req)?;
            let mut resp_lines = vec![format!("{:?} {}", resp.version(), resp.status())];
            resp.headers().iter().for_each(|(k, v)| {
                resp_lines.push(format!("{}: {}", k, v.to_str().unwrap_or_default()))
            });
            resp_lines.push("\r\n".to_string());
            stream.write_all(resp_lines.join("\r\n").as_bytes()).await?;
            tracing::debug!("{:?}", &resp);
            if resp.status() != http::StatusCode::SWITCHING_PROTOCOLS {
                return Err(WsError::HandShakeFailed(resp.body().to_string()));
            }
            codec_factory(req, remain, stream)
        }
    }
}

/// helper struct to config & construct websocket server
pub struct ServerBuilder {}

/// a trait that tells ws-tool corresponding opcode of custom type
pub trait DefaultCode {
    /// get payload opcode
    fn code(&self) -> OpCode;
}

impl DefaultCode for String {
    fn code(&self) -> OpCode {
        OpCode::Text
    }
}

impl DefaultCode for &[u8] {
    fn code(&self) -> OpCode {
        OpCode::Binary
    }
}
impl DefaultCode for &mut [u8] {
    fn code(&self) -> OpCode {
        OpCode::Binary
    }
}

impl DefaultCode for BytesMut {
    fn code(&self) -> OpCode {
        OpCode::Binary
    }
}

impl DefaultCode for Bytes {
    fn code(&self) -> OpCode {
        OpCode::Binary
    }
}

impl DefaultCode for ReadFrame {
    fn code(&self) -> OpCode {
        self.header().opcode()
    }
}

impl<'a> DefaultCode for BorrowedFrame<'a> {
    fn code(&self) -> OpCode {
        self.header().opcode()
    }
}

/// generic message receive/send from websocket stream
pub struct Message<T: AsRef<[u8]> + DefaultCode> {
    /// opcode of message
    ///
    /// see all codes in [overview](https://datatracker.ietf.org/doc/html/rfc6455#section-5.2) of opcode
    pub code: OpCode,
    /// payload of message
    pub data: T,

    /// available in close frame only
    ///
    /// see [status code](https://datatracker.ietf.org/doc/html/rfc6455#section-7.4)
    pub close_code: Option<u16>,
}

impl<T: AsRef<[u8]> + DefaultCode> Message<T> {
    /// consume message and return payload
    pub fn into(self) -> T {
        self.data
    }
}

impl<T: AsRef<[u8]> + DefaultCode> From<(OpCode, T)> for Message<T> {
    fn from(data: (OpCode, T)) -> Self {
        let close_code = if data.0 == OpCode::Close {
            Some(1000)
        } else {
            None
        };
        Self {
            data: data.1,
            code: data.0,
            close_code,
        }
    }
}

impl<T: AsRef<[u8]> + DefaultCode> From<(u16, T)> for Message<T> {
    fn from(data: (u16, T)) -> Self {
        Self {
            code: OpCode::Close,
            close_code: Some(data.0),
            data: data.1,
        }
    }
}

impl<T: AsRef<[u8]> + DefaultCode> From<T> for Message<T> {
    fn from(data: T) -> Self {
        Self {
            code: data.code(),
            data,
            close_code: None,
        }
    }
}
