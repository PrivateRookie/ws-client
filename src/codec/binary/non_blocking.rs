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

    pub fn new_with(stream: S, config: FrameConfig, read_bytes: BytesMut) -> Self {
        Self {
            frame_codec: AsyncWsFrameCodec::new_with(stream, config, read_bytes),
        }
    }

    pub fn factory(_req: http::Request<()>, remain: BytesMut, stream: S) -> Result<Self, WsError> {
        let config = FrameConfig {
            mask_send_frame: false,
            ..Default::default()
        };
        Ok(Self::new_with(stream, config, remain))
    }

    pub fn check_fn(
        key: String,
        resp: http::Response<()>,
        remain: BytesMut,
        stream: S,
    ) -> Result<Self, WsError> {
        standard_handshake_resp_check(key.as_bytes(), &resp)?;
        Ok(Self::new_with(stream, FrameConfig::default(), remain))
    }

    pub fn stream_mut(&mut self) -> &mut S {
        self.frame_codec.stream_mut()
    }

    pub async fn receive(&mut self) -> Result<Message<BytesMut>, WsError> {
        let frame = self.frame_codec.receive().await?;
        let header = frame.header();
        let header_len = header.payload_idx().0;
        let code = header.opcode();
        let mut data = frame.0;
        data.advance(header_len);
        let close_code = if code == OpCode::Close {
            Some(data.get_u16())
        } else {
            None
        };
        Ok(Message {
            code,
            data,
            close_code,
        })
    }

    pub async fn send<'a, T: Into<Message<&'a mut [u8]>>>(
        &mut self,
        msg: T,
    ) -> Result<(), WsError> {
        let msg: Message<&'a mut [u8]> = msg.into();
        if let Some(close_code) = msg.close_code {
            if msg.code == OpCode::Close {
                self.frame_codec
                    .send_mut(
                        msg.code,
                        vec![&mut close_code.to_be_bytes()[..], msg.data],
                        true,
                    )
                    .await
            } else {
                self.frame_codec.send_mut(msg.code, msg.data, true).await
            }
        } else {
            self.frame_codec.send_mut(msg.code, msg.data, true).await
        }
    }
}
