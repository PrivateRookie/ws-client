use std::{io::Write, path::PathBuf};

use futures::SinkExt;
use structopt::StructOpt;
use tokio_stream::StreamExt;
use tracing::Level;
use tracing_subscriber::util::SubscriberInitExt;
use ws_tool::{codec::default_string_check_fn, ClientBuilder};

/// websocket client demo with raw frame
#[derive(StructOpt)]
struct Args {
    uri: String,
    /// cert file path
    #[structopt(short, long)]
    cert: Option<PathBuf>,

    /// proxy setting
    #[structopt(long)]
    proxy: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    tracing_subscriber::fmt::fmt()
        .with_max_level(Level::DEBUG)
        .finish()
        .try_init()
        .expect("failed to init log");
    let args = Args::from_args();
    let mut builder = ClientBuilder::new(&args.uri);
    if let Some(cert) = args.cert {
        builder = builder.cert(cert);
    }
    if let Some(proxy) = args.proxy {
        builder = builder.proxy(&proxy)
    }
    let mut client = builder
        .connect_with_check(default_string_check_fn)
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
        if let Some(Ok((_, msg))) = client.next().await {
            println!("[RECV] > {}", msg.trim());
            if &msg == "quit" {
                break;
            }
            input.clear()
        } else {
            break;
        }
    }
    Ok(())
}
