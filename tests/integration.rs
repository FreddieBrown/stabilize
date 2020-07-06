extern crate stabilize;
use anyhow::{anyhow, Result};
use futures::StreamExt;
use stabilize::backend;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use std::path::PathBuf;
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
    let mut server_conn = match backend::ServerConnect::start(&socket_addr).await {
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

#[tokio::test]
async fn test_client_to_server() -> Result<()> {
    // Start up server
    tokio::spawn(async move {
        server::run(false, 5347).await.unwrap();
    });

    // Set up client and direct traffic towards stabilize
    // Only send one message

    tokio::spawn(async move {
        tokio::time::delay_for(Duration::new(2, 0)).await;
        let client = client::QuicClient::new_insecure("127.0.0.1:5000")
            .await
            .unwrap();
        for _ in 1..2 {
            let mut requests = client::generate_futures(&client);

            let mut finished_ops = 0;
            let max_finished_ops = 1;

            while finished_ops < max_finished_ops {
                match requests.next().await {
                    Some(s) => {
                        finished_ops += 1;
                        // Assert that message received back is what was set in server
                        assert_eq!("Returned", s.unwrap())
                    }
                    None => println!(
                        "Finished the stream before getting enough ops: {} vs {}",
                        finished_ops, max_finished_ops
                    ),
                }
            }
        }

        client.close();
    });

    // Re-create frontend run in test and make that run
    let server_config = stabilize::config_builder_raw(
        Some(PathBuf::from("signed.pem")),
        Some(PathBuf::from("signed.key")),
        true,
    )
    .await?;
    tokio::try_join!(stabilize::frontend::build_and_run_test_server(
        5000,
        server_config.clone()
    ))?;

    Ok(())
}
