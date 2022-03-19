use std::sync::mpsc;

use bytes::BytesMut;
use structopt::StructOpt;
use tracing_subscriber::util::SubscriberInitExt;
use ws_tool::{
    codec::{default_handshake_handler, WsBytesCodec},
    frame::OpCode,
    Message, ServerBuilder,
};

/// websocket client connect to binance futures websocket
#[derive(StructOpt)]
struct Args {
    /// server host
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,
    /// server port
    #[structopt(short, long, default_value = "9000")]
    port: u16,

    /// level
    #[structopt(short, long, default_value = "info")]
    level: tracing::Level,
}

fn main() -> Result<(), ()> {
    let args = Args::from_args();
    tracing_subscriber::fmt::fmt()
        .with_max_level(args.level)
        .finish()
        .try_init()
        .expect("failed to init log");
    tracing::info!("binding on {}:{}", args.host, args.port);
    let listener = std::net::TcpListener::bind(format!("{}:{}", args.host, args.port)).unwrap();
    loop {
        let (stream, addr) = listener.accept().unwrap();
        stream.set_nodelay(true).unwrap();
        std::thread::spawn(move || {
            tracing::info!("got connect from {:?}", addr);
            let server =
                ServerBuilder::accept(stream, default_handshake_handler, WsBytesCodec::factory)
                    .unwrap();
            let (mut read, mut write) = server.split();
            let (tx, rx) = mpsc::channel::<Message<BytesMut>>();
            std::thread::spawn(move || loop {
                if let Ok(msg) = read.receive() {
                    if msg.code == OpCode::Close {
                        break;
                    }
                    tx.send(msg).unwrap();
                }
            });

            loop {
                match rx.recv() {
                    Ok(mut msg) => {
                        write.send(&mut msg.data[..]).unwrap();
                    }
                    Err(e) => {
                        dbg!(e);
                        break;
                    }
                }
            }
            tracing::info!("one conn down");
        });
    }
}