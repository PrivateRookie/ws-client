#![allow(dead_code)]

use crate::errors::{ProtocolError, WsError};
use crate::frame::{Frame, OpCode};
use crate::protocol::standard_handshake_resp_check;
use crate::stream::WsStream;
use bytes::BytesMut;
use flate2::{Compress, Compression, Decompress, FlushCompress};
use tracing::debug;

use std::fmt::Debug;
use tokio::io::{ReadHalf, WriteHalf};
use tokio_util::codec::{Decoder, Encoder, Framed, FramedRead, FramedWrite};

use super::{SplitSocket, WebSocketFrameCodec, WebSocketFrameDecoder, WebSocketFrameEncoder};

const EXT_ID: &str = "permessage-deflate";

#[repr(u8)]
#[derive(Debug, Clone)]
pub enum WindowBit {
    Eight = 8,
    Night,
    Ten,
    Eleven,
    Twelve,
    Thirteen,
    Fourteen,
    FifTeen,
}

impl Default for WindowBit {
    fn default() -> Self {
        Self::Eight
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeflateConfig {
    pub server_no_context_takeover: bool,
    pub client_no_context_takeover: bool,
    pub server_max_window_bits: Option<WindowBit>,
    pub client_mas_window_bits: Option<WindowBit>,
}
impl DeflateConfig {
    pub fn build_header(&self) -> String {
        let mut ext_header = vec![EXT_ID.to_string()];
        if self.server_no_context_takeover {
            ext_header.push("server_no_context_takeover".to_string());
        }
        if self.client_no_context_takeover {
            ext_header.push("client_no_context_takeover".to_string());
        }

        if let Some(bit) = self.server_max_window_bits.clone() {
            ext_header.push(format!("server_max_window_bits = {}", bit as u8))
        }
        if let Some(bit) = self.client_mas_window_bits.clone() {
            ext_header.push(format!("client_max_window_bits = {}", bit as u8))
        }
        ext_header.join(" ;")
    }
}

#[derive(Debug)]
pub struct WebSocketDeflateEncoder {
    pub enable: bool,
    pub deflate_config: DeflateConfig,
    pub frame_encoder: WebSocketFrameEncoder,
    pub compress: Compress,
}

#[derive(Debug)]
pub struct WebSocketDeflateDecoder {
    pub enable: bool,
    pub deflate_config: DeflateConfig,
    pub frame_decoder: WebSocketFrameDecoder,
    pub decompress: Decompress,
}

#[derive(Debug)]
pub struct WebSocketDeflateCodec {
    pub enable: bool,
    pub deflate_config: DeflateConfig,
    pub codec: WebSocketFrameCodec,
    pub compress: Compress,
    pub decompress: Decompress,
}

impl Default for WebSocketDeflateCodec {
    fn default() -> Self {
        Self {
            enable: Default::default(),
            deflate_config: Default::default(),
            codec: Default::default(),
            compress: Compress::new(Compression::fast(), true),
            decompress: Decompress::new(true),
        }
    }
}

fn encode_frame(compress: &mut Compress, enable: bool, item: (OpCode, BytesMut)) -> Frame {
    match &item.0 {
        OpCode::Text | OpCode::Binary if enable => {
            let mut compressed = Vec::with_capacity(100);
            let input = Vec::from(item.1.as_ref());
            compress
                .compress_vec(&input, &mut compressed, FlushCompress::Sync)
                .unwrap();
            for _ in 0..4 {
                compressed.pop();
            }
            let mut frame = Frame::new_with_payload(item.0, &compressed);
            frame.set_rsv1(true);
            frame
        }
        _ => Frame::new_with_payload(item.0, &item.1),
    }
}

impl Encoder<(OpCode, BytesMut)> for WebSocketDeflateEncoder {
    type Error = WsError;

    fn encode(&mut self, item: (OpCode, BytesMut), dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.frame_encoder
            .encode(encode_frame(&mut self.compress, self.enable, item), dst)
    }
}

impl Encoder<(OpCode, BytesMut)> for WebSocketDeflateCodec {
    type Error = WsError;

    fn encode(&mut self, item: (OpCode, BytesMut), dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.codec
            .encode(encode_frame(&mut self.compress, self.enable, item), dst)
    }
}

fn decode_deflate_frame(
    decompress: &mut Decompress,
    enable: bool,
    frame: Frame,
) -> Result<Option<(OpCode, BytesMut)>, WsError> {
    let op_code = frame.opcode();
    let compressed = frame.rsv1();

    if !(op_code == OpCode::Text || op_code == OpCode::Binary) && compressed {
        if !enable {
            return Err(WsError::ProtocolError {
                close_code: 1002,
                error: ProtocolError::NotDeflateDataWhileEnabled,
            });
        }
        return Err(WsError::ProtocolError {
            close_code: 1002,
            error: ProtocolError::InvalidOpcode(op_code as u8),
        });
    }
    if compressed {
        let mut data = vec![];
        let mut input = frame.payload_data_unmask().to_vec();
        tracing::debug!("{:?}, {:x?}", frame, input);
        input.extend([0x00, 0x00, 0xff, 0xff]);
        decompress
            .decompress_vec(&input, &mut data, flate2::FlushDecompress::Finish)
            .unwrap();
        Ok(Some((op_code, BytesMut::from(&data[..]))))
    } else {
        let mut data = BytesMut::new();
        data.extend_from_slice(&frame.payload_data_unmask());
        Ok(Some((op_code, data)))
    }
}

impl Decoder for WebSocketDeflateDecoder {
    type Item = (OpCode, BytesMut);

    type Error = WsError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(frame) = self.frame_decoder.decode(src)? {
            decode_deflate_frame(&mut self.decompress, self.enable, frame)
        } else {
            Ok(None)
        }
    }
}

impl Decoder for WebSocketDeflateCodec {
    type Item = (OpCode, BytesMut);

    type Error = WsError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(frame) = self.codec.decode(src)? {
            decode_deflate_frame(&mut self.decompress, self.enable, frame)
        } else {
            Ok(None)
        }
    }
}

pub fn default_deflate_check_fn(
    key: String,
    resp: http::Response<()>,
    stream: WsStream,
) -> Result<Framed<WsStream, WebSocketDeflateCodec>, WsError> {
    standard_handshake_resp_check(key.as_bytes(), &resp)?;
    let enable = if let Some(ext) = resp.headers().get("Sec-WebSocket-Extensions") {
        let ext = ext.to_str().unwrap_or_default().to_lowercase();
        if ext.contains(EXT_ID) {
            // TODO handle more params
            // let deflate_ext: Vec<Vec<&str>> = ext
            //     .split(",")
            //     .filter(|seg| seg.contains(EXT_ID))
            //     .map(|seg| seg.split(";").map(|i| i.trim()).collect())
            //     .collect();
            true
        } else {
            tracing::debug!("server not support per message deflate");
            false
        }
    } else {
        false
    };
    let mut codec = WebSocketDeflateCodec {
        enable,
        ..Default::default()
    };
    codec.codec.config.check_rsv = false;
    debug!("{:#?}", codec);
    Ok(Framed::new(stream, codec))
}

pub fn default_bytes_codec_factory(
    req: http::Request<()>,
    stream: WsStream,
) -> Result<Framed<WsStream, WebSocketDeflateCodec>, WsError> {
    let enable = if let Some(ext) = req.headers().get("Sec-WebSocket-Extensions") {
        ext.to_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains(EXT_ID)
    } else {
        false
    };
    let mut codec = WebSocketDeflateCodec {
        enable,
        ..Default::default()
    };
    codec.codec.config.mask = false;
    codec.codec.config.check_rsv = false;
    Ok(Framed::new(stream, codec))
}

impl
    SplitSocket<
        (OpCode, BytesMut),
        (OpCode, BytesMut),
        WebSocketDeflateEncoder,
        WebSocketDeflateDecoder,
    > for Framed<WsStream, WebSocketDeflateCodec>
{
    fn split(
        self,
    ) -> (
        FramedRead<ReadHalf<WsStream>, WebSocketDeflateDecoder>,
        FramedWrite<WriteHalf<WsStream>, WebSocketDeflateEncoder>,
    ) {
        let parts = self.into_parts();
        let (read_io, write_io) = tokio::io::split(parts.io);
        let codec = parts.codec.codec;
        let mut frame_read = FramedRead::new(
            read_io,
            WebSocketDeflateDecoder {
                frame_decoder: WebSocketFrameDecoder {
                    config: codec.config.clone(),
                    fragmented: codec.fragmented,
                    fragmented_data: codec.fragmented_data,
                    fragmented_type: codec.fragmented_type,
                },
                decompress: parts.codec.decompress,
                enable: parts.codec.enable,
                deflate_config: parts.codec.deflate_config.clone(),
            },
        );
        *frame_read.read_buffer_mut() = parts.read_buf;
        let mut frame_write = FramedWrite::new(
            write_io,
            WebSocketDeflateEncoder {
                frame_encoder: WebSocketFrameEncoder {
                    config: codec.config,
                },
                enable: parts.codec.enable,
                deflate_config: parts.codec.deflate_config,
                compress: parts.codec.compress,
            },
        );
        *frame_write.write_buffer_mut() = parts.write_buf;
        (frame_read, frame_write)
    }
}
