extern crate stabilize;
use anyhow::{anyhow, Result};
use futures::StreamExt;
use stabilize::backend;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
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
    tokio::spawn(async move {
        server::run(false, 5378, 6378).await.unwrap();
    });

    println!("(Stabilize Test) About to run client socket");
    let mut incoming = bytes::BytesMut::new();
    // Create socket addr for port 5378
    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5378);
    println!(
        "(Stabilize Test) Server given from server pool: {}",
        socket_addr
    );
    tokio::time::delay_for(Duration::new(5, 0)).await;
    let mut server_conn =
        match backend::ServerConnect::start(&socket_addr, String::from("cstm-01")).await {
            Ok(conn) => conn,
            Err(_) => panic!("(Stabilize Test) Server isn't alive"),
        };
    let (mut send, mut recv) = match server_conn.connect().await {
        Ok(s) => s,
        Err(_) => panic!("(Stabilize Test) Cannot get streams for server connection"),
    };
    println!("(Stabilize Test) Connected to server");
    let mut recv_buffer = [0 as u8; 1024];
    let mut msg_size = 0;
    let body = "test123".as_bytes();
    send.write_all(&body).await.unwrap();
    send.finish().await.unwrap();
    println!("(Stabilize Test) Written to Server");

    while let Some(s) = recv
        .read(&mut recv_buffer)
        .await
        .map_err(|e| {
            anyhow!(
                "(Stabilize Test) Could not read message from recv stream: {}",
                e
            )
        })
        .unwrap()
    {
        msg_size += s;
        incoming.extend_from_slice(&recv_buffer[0..s]);
        println!(
            "(Stabilize Test) Received from stream: {}",
            std::str::from_utf8(&recv_buffer[0..s]).unwrap()
        );
    }
    assert_eq!(
        "Returned",
        std::str::from_utf8(&recv_buffer[0..msg_size]).unwrap()
    );
    println!("(Stabilize Test) Client shutting down");
    Ok(())
}

#[tokio::test]
async fn test_client_to_server() -> Result<()> {
    // Start up server
    tokio::spawn(async move {
        server::run(false, 5347, 6347).await.unwrap();
    });

    // Set up client and direct traffic towards stabilize
    // Only send one message

    tokio::spawn(async move {
        tokio::time::delay_for(Duration::new(3, 0)).await;
        let client = client::QuicClient::new("127.0.0.1:5000").await.unwrap();
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
                        "(Stabilize Test) Finished the stream before getting enough ops: {} vs {}",
                        finished_ops, max_finished_ops
                    ),
                }
            }
        }

        client.close();
    });

    // Re-create frontend run in test and make that run
    let server_config = stabilize::config_builder(None, None, &[b"cstm-01"]).await?;
    tokio::try_join!(stabilize::frontend::build_and_run_test_server(
        5000,
        server_config.clone(),
        "test_data/test_config1.toml",
        String::from("cstm-01")
    ))?;

    Ok(())
}
