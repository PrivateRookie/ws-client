#[cfg(feature = "blocking")]
mod blocking {

    #[cfg(feature = "tls_rustls")]
    mod stream {
        use rustls_connector::TlsStream;
        use std::io::{Read, Write};

        pub enum WsStream<S: Read + Write> {
            Plain(S),
            Tls(TlsStream<S>),
        }

        impl<S: Read + Write> WsStream<S> {
            pub fn stream_mut(&mut self) -> &mut S {
                match self {
                    WsStream::Plain(s) => s,
                    WsStream::Tls(tls) => tls.get_mut(),
                }
            }
        }

        impl<S: Read + Write> Read for WsStream<S> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                match self {
                    WsStream::Plain(s) => s.read(buf),
                    WsStream::Tls(s) => s.read(buf),
                }
            }
        }

        impl<S: Read + Write> Write for WsStream<S> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                match self {
                    WsStream::Plain(s) => s.write(buf),
                    WsStream::Tls(s) => s.write(buf),
                }
            }

            fn flush(&mut self) -> std::io::Result<()> {
                match self {
                    WsStream::Plain(s) => s.flush(),
                    WsStream::Tls(s) => s.flush(),
                }
            }
        }
    }

    #[cfg(not(feature = "tls_rustls"))]
    mod stream {
        use std::{
            io::{Read, Write},
        };

        pub enum WsStream<S> {
            Plain(S),
        }

        impl<S> WsStream<S> {
            pub fn stream_mut(&mut self) -> &mut S {
                match self {
                    WsStream::Plain(s) => s,
                }
            }
        }

        impl<S: Read> Read for WsStream<S> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                match self {
                    WsStream::Plain(s) => s.read(buf),
                }
            }
        }

        impl<S: Write> Write for WsStream<S> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                match self {
                    WsStream::Plain(s) => s.write(buf),
                }
            }

            fn flush(&mut self) -> std::io::Result<()> {
                match self {
                    WsStream::Plain(s) => s.flush(),
                }
            }
        }
    }

    pub use stream::WsStream;
}

#[cfg(feature = "blocking")]
pub use blocking::WsStream;

#[cfg(feature = "async")]
mod non_blocking {
    #[cfg(feature = "async_tls_rustls")]
    mod ws_stream {
        use tokio::io::{AsyncRead, AsyncWrite};
        use tokio_rustls::client::TlsStream;

        #[derive(Debug)]
        pub enum WsAsyncStream<S: AsyncRead + AsyncWrite> {
            Plain(S),
            Tls(TlsStream<S>),
        }

        impl<S: AsyncWrite + AsyncRead> WsAsyncStream<S> {
            pub fn stream_mut(&mut self) -> &mut S {
                match self {
                    WsAsyncStream::Plain(s) => s,
                    WsAsyncStream::Tls(s) => s.get_mut().0,
                }
            }
        }

        impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for WsAsyncStream<S> {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
                    WsAsyncStream::Tls(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
                }
            }
        }

        impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for WsAsyncStream<S> {
            fn poll_write(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
                    WsAsyncStream::Tls(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
                }
            }

            fn poll_flush(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_flush(cx),
                    WsAsyncStream::Tls(stream) => std::pin::Pin::new(stream).poll_flush(cx),
                }
            }

            fn poll_shutdown(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
                    WsAsyncStream::Tls(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
                }
            }
        }
    }

    #[cfg(not(feature = "async_tls_rustls"))]
    mod ws_stream {
        use tokio::io::{AsyncRead, AsyncWrite};

        #[derive(Debug)]
        pub enum WsAsyncStream<S> {
            Plain(S),
        }

        impl<S> WsAsyncStream<S> {
            pub fn stream_mut(&mut self) -> &mut S {
                match self {
                    Self::Plain(s) => s,
                }
            }
        }

        impl<S: AsyncRead + Unpin> AsyncRead for WsAsyncStream<S> {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
                }
            }
        }

        impl<S: AsyncWrite + Unpin> AsyncWrite for WsAsyncStream<S> {
            fn poll_write(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
                }
            }

            fn poll_flush(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_flush(cx),
                }
            }

            fn poll_shutdown(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                match self.get_mut() {
                    WsAsyncStream::Plain(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
                }
            }
        }
    }

    pub use ws_stream::WsAsyncStream;
}

#[cfg(feature = "async")]
pub use non_blocking::WsAsyncStream;
