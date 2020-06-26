#![warn(rust_2018_idioms)]

use std::error::Error;
use std::net::SocketAddr;
use std::env;
use tokio::net::UdpSocket;
// use tokio::time::{Duration, Instant, delay_for};
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};
// use tokio::net::udp::{RecvHalf, SendHalf};
use futures::StreamExt;

mod client;
mod server;


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let server_addr = "127.0.0.1:8080";
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    
    let server_addr: SocketAddr = server_addr.parse::<SocketAddr>()?;

    let socket = UdpSocket::bind(&addr).await?;
    println!("Listening on: {}", socket.local_addr()?);

    let client = client::Client::new(server_addr).await?;
    let server = server::Server::new(socket, vec![0; 1024], None);

    let stdin = FramedRead::new(tokio::io::stdin(), BytesCodec::new());
    let stdin = stdin.map(|i| i.map(|bytes| bytes.freeze()));
    let stdout = FramedWrite::new(tokio::io::stdout(), BytesCodec::new());

    // This starts the server task.
    tokio::spawn(async move {
        server.run().await.unwrap();
    });

    client.run(stdin,stdout).await?;

    Ok(())
}
