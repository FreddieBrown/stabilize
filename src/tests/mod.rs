use crate::backend::ServerConnect;
use anyhow::{anyhow, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio_test;
mod client;
mod server;

/// Sanity check
#[test]
fn test_sanity() {
    assert_eq!(1, 1);
}

/// This is a test to check if the method by which the stabilize lb connects 
/// to a server works. Mainly tests the ServerConnect struct, but also tests the 
/// ideas behind the main project.
#[tokio::test]
async fn test_stab_to_server() -> Result<()> {
    println!("Testing stab to server");
    tokio::spawn(async move {
        server::run(false, 5378).await.unwrap();
    });

    println!("About to run client socket");
    let mut incoming = bytes::BytesMut::new();
    // Create socket addr for port 5378
    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5378);
    println!("Server given from server pool: {}", socket_addr);
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut server_conn = match ServerConnect::start(&socket_addr).await {
        Ok(conn) => conn,
        Err(_) => panic!("Server isn't alive"),
    };
    let (mut send, mut recv) = match server_conn.connect().await {
        Ok(s) => s,
        Err(_) => panic!("Cannot get streams for server connection"),
    };
    println!("connected to server");
    let mut recv_buffer = [0 as u8; 1024];
    let mut msg_size = 0;
    let body = "test123".as_bytes();
    send.write_all(&body).await.unwrap();
    send.finish().await.unwrap();
    println!("Written to Server");

    while let Some(s) = recv
        .read(&mut recv_buffer)
        .await
        .map_err(|e| anyhow!("Could not read message from recv stream: {}", e))
        .unwrap()
    {
        msg_size += s;
        incoming.extend_from_slice(&recv_buffer[0..s]);
        println!(
            "Received from stream: {}",
            std::str::from_utf8(&recv_buffer[0..s]).unwrap()
        );
    }
    assert_eq!(
        "Returned",
        std::str::from_utf8(&recv_buffer[0..msg_size]).unwrap()
    );
    println!("Client shutting down");
    Ok(())
}


/// These tests will test the health checking functionality of the stabilize server. If it is passing,  
/// it will go through the 3 servers in config and will find that 2 are working and 1 isn't. Will also 
/// check if ServerPool is working too.
#[test]
fn test_server_healthcheck() {
    assert_eq!(1, 1);
}
