#[cfg(feature = "blocking")]
mod blocking {
    use std::io::{Read, Write};

    use bytes::Buf;

    use crate::{
        codec::{FrameConfig, WsFrameCodec},
        errors::{ProtocolError, WsError},
        frame::OpCode,
        protocol::standard_handshake_resp_check,
        Message,
    };

    pub struct WsStringCodec<S: Read + Write> {
        frame_codec: WsFrameCodec<S>,
        validate_utf8: bool,
    }

    impl<S: Read + Write> WsStringCodec<S> {
        pub fn new(stream: S) -> Self {
            Self {
                frame_codec: WsFrameCodec::new(stream),
                validate_utf8: false,
            }
        }

        pub fn new_with(stream: S, config: FrameConfig, validate_utf8: bool) -> Self {
            Self {
                frame_codec: WsFrameCodec::new_with(stream, config),
                validate_utf8,
            }
        }

        pub fn stream_mut(&mut self) -> &mut S {
            self.frame_codec.stream_mut()
        }

        pub fn check_fn(key: String, resp: http::Response<()>, stream: S) -> Result<Self, WsError> {
            standard_handshake_resp_check(key.as_bytes(), &resp)?;
            Ok(Self::new_with(stream, FrameConfig::default(), true))
        }

        pub fn factory(_req: http::Request<()>, stream: S) -> Result<Self, WsError> {
            let mut config = FrameConfig::default();
            config.mask = false;
            Ok(Self::new_with(stream, config, true))
        }

        /// for close frame with body, first two bytes of string are close reason
        pub fn receive(&mut self) -> Result<Message<String>, WsError> {
            let frame = self.frame_codec.receive()?;
            let mut data = frame.payload_data_unmask();
            let close_code = if frame.opcode() == OpCode::Close {
                let close_code = data.get_u16();
                Some(close_code)
            } else {
                None
            };
            let data = if self.validate_utf8 && frame.opcode() == OpCode::Text {
                String::from_utf8(data.to_vec()).map_err(|_| WsError::ProtocolError {
                    close_code: 1001,
                    error: ProtocolError::InvalidUtf8,
                })?
            } else {
                String::from_utf8_lossy(&data).to_string()
            };
            Ok(Message {
                data,
                close_code,
                code: frame.opcode(),
            })
        }

        pub fn send<T: Into<Message<String>>>(&mut self, msg: T) -> Result<usize, WsError> {
            let msg: Message<String> = msg.into();
            if let Some(close_code) = msg.close_code {
                if msg.code == OpCode::Close {
                    self.frame_codec.send(
                        msg.code,
                        vec![&close_code.to_be_bytes()[..], msg.data.as_bytes()],
                    )
                } else {
                    self.frame_codec.send(msg.code, msg.data.as_bytes())
                }
            } else {
                self.frame_codec.send(msg.code, msg.data.as_bytes())
            }
        }
    }
}

#[cfg(feature = "blocking")]
pub use blocking::WsStringCodec;

#[cfg(feature = "async")]
mod non_blocking {
    use bytes::Buf;
    use tokio::io::{AsyncRead, AsyncWrite};

    use crate::{
        codec::{AsyncWsFrameCodec, FrameConfig},
        errors::{ProtocolError, WsError},
        frame::OpCode,
        protocol::standard_handshake_resp_check,
        Message,
    };

    pub struct AsyncWsStringCodec<S: AsyncRead + AsyncWrite> {
        frame_codec: AsyncWsFrameCodec<S>,
        validate_utf8: bool,
    }

    impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWsStringCodec<S> {
        pub fn new(stream: S) -> Self {
            Self {
                frame_codec: AsyncWsFrameCodec::new(stream),
                validate_utf8: false,
            }
        }

        pub fn new_with(stream: S, config: FrameConfig, validate_utf8: bool) -> Self {
            Self {
                frame_codec: AsyncWsFrameCodec::new_with(stream, config),
                validate_utf8,
            }
        }

        pub fn stream_mut(&mut self) -> &mut S {
            self.frame_codec.stream_mut()
        }

        pub fn check_fn(key: String, resp: http::Response<()>, stream: S) -> Result<Self, WsError> {
            standard_handshake_resp_check(key.as_bytes(), &resp)?;
            Ok(Self::new_with(stream, FrameConfig::default(), true))
        }

        pub fn factory(_req: http::Request<()>, stream: S) -> Result<Self, WsError> {
            let mut config = FrameConfig::default();
            config.mask = false;
            Ok(Self::new_with(stream, config, true))
        }

        pub async fn receive(&mut self) -> Result<Message<String>, WsError> {
            let frame = self.frame_codec.receive().await?;
            let mut data = frame.payload_data_unmask();
            // TODO check protocol error
            let close_code = if frame.opcode() == OpCode::Close && data.len() >= 2 {
                let close_code = data.get_u16();
                Some(close_code)
            } else {
                None
            };
            let data = if self.validate_utf8 && frame.opcode() == OpCode::Text {
                String::from_utf8(data.to_vec()).map_err(|_| WsError::ProtocolError {
                    close_code: 1001,
                    error: ProtocolError::InvalidUtf8,
                })?
            } else {
                String::from_utf8_lossy(&data).to_string()
            };
            Ok(Message {
                data,
                close_code,
                code: frame.opcode(),
            })
        }

        pub async fn send<T: Into<Message<String>>>(&mut self, msg: T) -> Result<usize, WsError> {
            let msg: Message<String> = msg.into();
            if let Some(close_code) = msg.close_code {
                if msg.code == OpCode::Close {
                    self.frame_codec
                        .send(
                            msg.code,
                            vec![&close_code.to_be_bytes()[..], msg.data.as_bytes()],
                        )
                        .await
                } else {
                    self.frame_codec.send(msg.code, msg.data.as_bytes()).await
                }
            } else {
                self.frame_codec.send(msg.code, msg.data.as_bytes()).await
            }
        }
    }
}

#[cfg(feature = "async")]
pub use non_blocking::AsyncWsStringCodec;
