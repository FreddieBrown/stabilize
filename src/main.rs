#![warn(rust_2018_idioms)]

use std::error::Error;
use std::net::SocketAddr;
use std::env;
use tokio::net::UdpSocket;
use std::str;
use std::collections::HashMap;

mod server;
// mod listener;


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    // Build Vec of servers
    let mut clients = server::Server::create_from_file();
    println!("{:?}", &clients);
    let mut counter = 0;

    // Create UDP listener for clients
    let mut socket = UdpSocket::bind(&addr).await?;
    println!("Listening on: {}", socket.local_addr()?);
    let mut buf = [0; 1024];
    let mut to_send: Option<(usize, SocketAddr)> = None; 
    loop {  
        // When data has been received over the socket, the packet will be dealt with
        // Here, it will be sent onto a server connected to the 
        if let Some((size, peer)) = to_send {
            // Stabilize will take packet, and send it onto one of its servers. IN PROGRESS
            println!("Data Received from {}: {}", &peer, str::from_utf8(&buf[..size])?);
            let (client, mut val) = clients[counter];
            val = Some(peer);
            clients[counter] = (client, val);
            counter += 1;
            if counter == clients.len() {
                counter = 0;
            }
        }
        println!("{:?}", &clients);
        // Stabilize listens for data over the socket
        to_send = Some(socket.recv_from(&mut buf).await.unwrap());
    }

}
