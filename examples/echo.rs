use std::{io::Write, path::PathBuf};

use clap::Parser;
use tracing::Level;
use tracing_subscriber::util::SubscriberInitExt;
use ws_tool::{codec::AsyncWsStringCodec, ClientBuilder};

/// websocket client demo with raw frame
#[derive(Parser)]
struct Args {
    uri: String,
    /// cert file path
    #[clap(short, long)]
    cert: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    tracing_subscriber::fmt::fmt()
        .with_max_level(Level::DEBUG)
        .finish()
        .try_init()
        .expect("failed to init log");
    let args = Args::parse();
    let mut builder = ClientBuilder::new(&args.uri);
    if let Some(cert) = args.cert {
        builder = builder.cert(cert);
    }
    // if let Some(proxy) = args.proxy {
    //     builder = builder.proxy(&proxy)
    // }
    let mut client = builder
        .async_connect(AsyncWsStringCodec::check_fn)
        .await
        .unwrap();

    let mut input = String::new();
    loop {
        print!("[SEND] > ");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_line(&mut input).unwrap();
        if &input == "quit\n" {
            break;
        }
        client.send(input.clone()).await.unwrap();
        match client.receive().await {
            Ok(item) => {
                println!("[RECV] > {}", item.data.trim());
                if item.data == "quit" {
                    break;
                }
                input.clear()
            }
            Err(e) => {
                dbg!(e);
                break;
            }
        }
    }
    Ok(())
}
