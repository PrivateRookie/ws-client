use std::io::{Read, Write};

use crate::{
    codec::{FrameCodec, FrameConfig, ValidateUtf8Policy},
    errors::{ProtocolError, WsError},
    frame::OwnedFrame,
    protocol::standard_handshake_resp_check,
};

use super::{Compressor, DeCompressor, PMDConfig, StreamHandler};

/// recv/send deflate message
pub struct DeflateCodec<S: Read + Write> {
    frame_codec: FrameCodec<S>,
    stream_handler: Option<StreamHandler>,
    is_server: bool,
}

impl<S: Read + Write> DeflateCodec<S> {
    /// construct method
    pub fn new(frame_codec: FrameCodec<S>, config: Option<PMDConfig>, is_server: bool) -> Self {
        let stream_handler = if let Some(config) = config {
            let com_size = if is_server {
                config.client_max_window_bits
            } else {
                config.server_max_window_bits
            };
            let com = Compressor::new(com_size);
            let de_size = if is_server {
                config.client_max_window_bits
            } else {
                config.server_max_window_bits
            };
            let de = DeCompressor::new(de_size);
            Some(StreamHandler { config, com, de })
        } else {
            None
        };
        Self {
            frame_codec,
            stream_handler,
            is_server,
        }
    }

    /// used for server side to construct a new server
    pub fn factory(req: http::Request<()>, stream: S) -> Result<Self, WsError> {
        let frame_config = FrameConfig {
            mask_send_frame: false,
            check_rsv: false,
            validate_utf8: ValidateUtf8Policy::Off,
            ..Default::default()
        };
        let mut configs: Vec<PMDConfig> = vec![];
        for (k, v) in req.headers() {
            if k.as_str().to_lowercase() == "sec-websocket-extensions" {
                if let Ok(s) = v.to_str() {
                    match PMDConfig::parse_str(s) {
                        Ok(mut conf) => {
                            configs.append(&mut conf);
                        }
                        Err(e) => return Err(WsError::HandShakeFailed(e)),
                    }
                }
            }
        }
        let mut config = configs.pop();
        if let Some(conf) = config.as_mut() {
            let min = conf.client_max_window_bits.min(conf.server_max_window_bits);
            conf.client_max_window_bits = min;
            conf.server_max_window_bits = min;
        }
        tracing::debug!("use deflate config {:?}", config);
        let frame_codec = FrameCodec::new_with(stream, frame_config);
        let codec = DeflateCodec::new(frame_codec, config, true);
        Ok(codec)
    }

    /// used for client side to construct a new client
    pub fn check_fn(key: String, resp: http::Response<()>, stream: S) -> Result<Self, WsError> {
        standard_handshake_resp_check(key.as_bytes(), &resp)?;
        let mut configs: Vec<PMDConfig> = vec![];
        for (k, v) in resp.headers() {
            if k.as_str().to_lowercase() == "sec-websocket-extensions" {
                if let Ok(s) = v.to_str() {
                    match PMDConfig::parse_str(s) {
                        Ok(mut conf) => {
                            configs.append(&mut conf);
                        }
                        Err(e) => return Err(WsError::HandShakeFailed(e)),
                    }
                }
            }
        }
        let mut config = configs.pop();
        if let Some(conf) = config.as_mut() {
            let min = conf.client_max_window_bits.min(conf.server_max_window_bits);
            conf.client_max_window_bits = min;
            conf.server_max_window_bits = min;
        }
        let frame_codec = FrameCodec::new_with(
            stream,
            FrameConfig {
                check_rsv: false,
                mask_send_frame: false,
                validate_utf8: ValidateUtf8Policy::Off,
                ..Default::default()
            },
        );
        tracing::debug!("use deflate config: {:?}", config);
        let codec = DeflateCodec::new(frame_codec, config, false);
        Ok(codec)
    }

    /// get mutable underlying stream
    pub fn stream_mut(&mut self) -> &mut S {
        self.frame_codec.stream_mut()
    }

    /// receive a message
    pub fn receive(&mut self) -> Result<OwnedFrame, WsError> {
        let frame = self.frame_codec.receive()?;
        let compressed = frame.header().rsv1();
        let is_data_frame = frame.header().opcode().is_data();
        if compressed && !is_data_frame {
            return Err(WsError::ProtocolError {
                close_code: 1002,
                error: ProtocolError::CompressedControlFrame,
            });
        }
        if !is_data_frame {
            return Ok(frame);
        }
        let frame: OwnedFrame = match self.stream_handler.as_mut() {
            Some(handler) => {
                let mut decompressed = Vec::with_capacity(frame.payload().len() * 2);
                let (header, mut payload) = frame.parts();
                payload.extend_from_slice(&[0, 0, 255, 255]);
                handler
                    .de
                    .decompress(&payload, &mut decompressed)
                    .map_err(WsError::DeCompressFailed)?;
                if (self.is_server && handler.config.server_no_context_takeover)
                    || (!self.is_server && handler.config.client_no_context_takeover)
                {
                    handler.de.reset().map_err(WsError::DeCompressFailed)?;
                    tracing::trace!("reset decompressor state");
                }
                OwnedFrame::new(header.opcode(), None, &decompressed[..])
            }
            None => {
                if frame.header().rsv1() {
                    return Err(WsError::DeCompressFailed(
                        "extension not enabled but got compressed frame".into(),
                    ));
                } else {
                    frame
                }
            }
        };
        Ok(frame)
    }

    /// send a read frame, **this method will not check validation of frame and do not fragment**
    pub fn send_owned_frame(&mut self, mut frame: OwnedFrame) -> Result<(), WsError> {
        if !frame.header().opcode().is_data() {
            return self.frame_codec.send_owned_frame(frame);
        }
        let prev_mask = frame.unmask();
        let header = frame.header();
        let frame: Result<OwnedFrame, WsError> = self
            .stream_handler
            .as_mut()
            .map(|handler| {
                let mut compressed = Vec::with_capacity(frame.payload().len());
                handler
                    .com
                    .compress(frame.payload(), &mut compressed)
                    .map_err(WsError::CompressFailed)?;
                compressed.truncate(compressed.len() - 4);
                let mut new = OwnedFrame::new(header.opcode(), prev_mask, &compressed);
                let header = new.header_mut();
                header.set_rsv1(true);
                header.set_fin(header.fin());

                if (self.is_server && handler.config.server_no_context_takeover)
                    || (!self.is_server && handler.config.client_no_context_takeover)
                {
                    handler.com.reset().map_err(WsError::CompressFailed)?;
                    tracing::trace!("reset compressor");
                }
                Ok(new)
            })
            .unwrap_or_else(|| {
                if let Some(mask) = prev_mask {
                    frame.mask(mask);
                }
                Ok(frame)
            });
        self.frame_codec.send_owned_frame(frame?)
    }
}
