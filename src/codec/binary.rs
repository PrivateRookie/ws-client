#[cfg(feature = "blocking")]
mod blocking {
    use std::io::{Read, Write};

    use bytes::{Buf, BytesMut};

    use crate::{
        codec::{FrameConfig, WsFrameCodec},
        errors::WsError,
        frame::OpCode,
        protocol::standard_handshake_resp_check,
        Message,
    };

    pub struct WsBytesCodec<S: Read + Write> {
        frame_codec: WsFrameCodec<S>,
    }

    impl<S: Read + Write> WsBytesCodec<S> {
        pub fn new(stream: S) -> Self {
            Self {
                frame_codec: WsFrameCodec::new(stream),
            }
        }

        pub fn new_with(stream: S, config: FrameConfig) -> Self {
            Self {
                frame_codec: WsFrameCodec::new_with(stream, config),
            }
        }

        pub fn factory(_req: http::Request<()>, stream: S) -> Result<Self, WsError> {
            let mut config = FrameConfig::default();
            // do not mask server side frame
            config.mask = false;
            Ok(Self::new_with(stream, config))
        }

        pub fn check_fn(key: String, resp: http::Response<()>, stream: S) -> Result<Self, WsError> {
            standard_handshake_resp_check(key.as_bytes(), &resp)?;
            Ok(Self::new_with(stream, FrameConfig::default()))
        }

        pub fn stream_mut(&mut self) -> &mut S {
            self.frame_codec.stream_mut()
        }

        pub fn receive(&mut self) -> Result<Message<BytesMut>, WsError> {
            let frame = self.frame_codec.receive()?;
            let mut data = BytesMut::new();
            data.extend_from_slice(&frame.payload_data_unmask());
            let op_code = frame.opcode();
            let close_code = if op_code == OpCode::Close {
                Some(data.get_u16())
            } else {
                None
            };
            Ok(Message {
                code: frame.opcode(),
                data,
                close_code,
            })
        }

        pub fn send<'a, T: Into<Message<&'a [u8]>>>(&mut self, msg: T) -> Result<usize, WsError> {
            let msg: Message<&'a [u8]> = msg.into();
            if let Some(close_code) = msg.close_code {
                if msg.code == OpCode::Close {
                    self.frame_codec
                        .send(msg.code, vec![&close_code.to_be_bytes()[..], msg.data])
                } else {
                    self.frame_codec.send(msg.code, msg.data)
                }
            } else {
                self.frame_codec.send(msg.code, msg.data)
            }
        }
    }
}

#[cfg(feature = "blocking")]
pub use blocking::WsBytesCodec;

#[cfg(feature = "async")]
mod non_blocking {
    use bytes::{Buf, BytesMut};
    use tokio::io::{AsyncRead, AsyncWrite};

    use crate::{
        codec::{AsyncWsFrameCodec, FrameConfig},
        errors::WsError,
        frame::OpCode,
        protocol::standard_handshake_resp_check,
        Message,
    };

    pub struct AsyncWsBytesCodec<S: AsyncRead + AsyncWrite> {
        frame_codec: AsyncWsFrameCodec<S>,
    }

    impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWsBytesCodec<S> {
        pub fn new(stream: S) -> Self {
            Self {
                frame_codec: AsyncWsFrameCodec::new(stream),
            }
        }

        pub fn new_with(stream: S, config: FrameConfig) -> Self {
            Self {
                frame_codec: AsyncWsFrameCodec::new_with(stream, config),
            }
        }

        pub fn factory(_req: http::Request<()>, stream: S) -> Result<Self, WsError> {
            let mut config = FrameConfig::default();
            // do not mask server side frame
            config.mask = false;
            Ok(Self::new_with(stream, config))
        }

        pub fn check_fn(key: String, resp: http::Response<()>, stream: S) -> Result<Self, WsError> {
            standard_handshake_resp_check(key.as_bytes(), &resp)?;
            Ok(Self::new_with(stream, FrameConfig::default()))
        }

        pub fn stream_mut(&mut self) -> &mut S {
            self.frame_codec.stream_mut()
        }

        pub async fn receive(&mut self) -> Result<Message<BytesMut>, WsError> {
            let frame = self.frame_codec.receive().await?;
            let mut data = BytesMut::new();
            data.extend_from_slice(&frame.payload_data_unmask());
            let op_code = frame.opcode();
            let close_code = if op_code == OpCode::Close {
                Some(data.get_u16())
            } else {
                None
            };
            Ok(Message {
                code: frame.opcode(),
                data,
                close_code,
            })
        }

        pub async fn send<'a, T: Into<Message<&'a [u8]>>>(
            &mut self,
            msg: T,
        ) -> Result<usize, WsError> {
            let msg: Message<&'a [u8]> = msg.into();
            if let Some(close_code) = msg.close_code {
                if msg.code == OpCode::Close {
                    self.frame_codec
                        .send(msg.code, vec![&close_code.to_be_bytes()[..], msg.data])
                        .await
                } else {
                    self.frame_codec.send(msg.code, msg.data).await
                }
            } else {
                self.frame_codec.send(msg.code, msg.data).await
            }
        }
    }
}

#[cfg(feature = "async")]
pub use non_blocking::AsyncWsBytesCodec;
