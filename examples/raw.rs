use std::{io::Write, path::PathBuf};

use structopt::StructOpt;
use ws_client::{frame::Frame, ClientBuilder};

/// websocket client demo with raw frame
#[derive(StructOpt)]
struct Args {
    uri: String,
    /// cert file path
    #[structopt(short, long)]
    cert: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    pretty_env_logger::init();
    let args = Args::from_args();
    let mut builder = ClientBuilder::new(&args.uri);
    if let Some(cert) = args.cert {
        builder = builder.cert(cert);
    }
    let mut client = builder.build().unwrap();
    client.connect().await.unwrap();

    let mut input = String::new();
    loop {
        print!("[SEND] > ");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_line(&mut input).unwrap();
        if &input == "quit\n" {
            println!("should exit");
            break;
        }
        let mut frame = Frame::new();
        frame.set_payload(input.trim().as_bytes());
        client.write_frame(frame).await.unwrap();
        let resp = client.read_frame().await.unwrap();
        let msg = String::from_utf8(resp.payload_data_unmask().to_vec()).unwrap();
        println!("[RECV] > {}", msg.trim());
        if &msg == "quit" {
            break;
        }
    }
    client.close().await.unwrap();
    return Ok(());
}
