use crate::backend::ServerConnect;
use crate::backend::ServerPool;
use anyhow::{anyhow, Result};
use tokio_test;

use structopt::{self, StructOpt};

use std::net::SocketAddr;

#[test]
fn test_sanity() {
    assert_eq!(1, 1);
}

#[tokio::test]
async fn test_stab_to_server() -> Result<()> {
    println!("About to run client socket");
    tokio::spawn(async move {
        // Need to write more server code to connect to client part of stab
        println!("Input test server code here!");
    });
    // Create a server for it to connect to
    let mut incoming = bytes::BytesMut::new();
    let serverpool = ServerPool::create_from_file();
    let server = serverpool.get_next().await;
    println!("Server given from server pool: {}", server.get_addr());
    let mut server_conn = match ServerConnect::start(&server.get_addr()).await {
        Ok(conn) => conn,
        Err(_) => panic!("Server isn't alive"),
    };
    let (mut send, mut recv) = match server_conn.connect().await {
        Ok(s) => s,
        Err(_) => panic!("Cannot get streams for server connection"),
    };
    println!("connected to server");
    let mut recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
    let mut msg_size = 0;
    let body = "Hello".as_bytes();
    send.write_all(&body);
    println!("Written to Server");

    while let Some(s) = recv
        .read(&mut recv_buffer)
        .await
        .map_err(|e| anyhow!("Could not read message from recv stream: {}", e))?
    {
        msg_size += s;
        incoming.extend_from_slice(&recv_buffer[0..s]);
        println!(
            "Received from stream: {}",
            std::str::from_utf8(&recv_buffer[0..s]).unwrap()
        );
    }

    Ok(())
}

#[test]
fn test_client_to_server() {
    assert_eq!(1, 1);
}

#[test]
fn test_server_healthcheck() {
    assert_eq!(1, 1);
}
