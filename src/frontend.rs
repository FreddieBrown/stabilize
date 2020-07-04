use std::{net::IpAddr, net::Ipv4Addr, net::SocketAddr};

use crate::backend::ServerConnect;
use crate::backend::ServerPool;
use anyhow::{anyhow, Result};
use futures::{StreamExt, TryFutureExt};
use quinn::ServerConfig;

pub async fn build_and_run_server(port: u16, server_config: ServerConfig) -> Result<()> {
    let mut endpoint_builder = quinn::Endpoint::builder();
    endpoint_builder.listen(server_config.clone());
    let serverpool = ServerPool::create_from_file();

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let mut incoming = {
        let (endpoint, incoming) = endpoint_builder.bind(&socket_addr)?;
        println!("Server listening on {}", endpoint.local_addr()?);
        incoming
    };

    while let Some(conn) = incoming.next().await {
        println!("{}: new connection!", socket_addr);
        let server = serverpool.get_next().await;
        println!("Server given from server pool: {}", server.get_addr());
        tokio::spawn(
            handle_conn(conn, server.get_addr()).unwrap_or_else(move |e| {
                println!("{}: connection failed: {}", socket_addr, e);
            }),
        );
    }

    Ok(())
}

async fn handle_conn(conn: quinn::Connecting, server: SocketAddr) -> Result<()> {
    let quinn::NewConnection {
        connection: _connection,
        mut bi_streams,
        ..
    } = conn.await?;

    // dispatch the actual handling as another future. concurrency!
    async {
        // each stream needs to be processed independently
        while let Some(stream) = bi_streams.next().await {
            let client_streams = match stream {
                Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                    // application closed, finish up early
                    return Ok(());
                }
                Err(e) => {
                    return Err(e);
                }
                Ok(s) => s,
            };
            // Connect to backend server
            let mut server_conn = match ServerConnect::start(&server).await {
                Ok(conn) => conn,
                Err(_) => panic!("Server isn't alive"),
            };
            // Spawn message handling task
            tokio::spawn(
                handle_response(client_streams, server_conn)
                    .unwrap_or_else(move |e| eprintln!("Response failed: {}", e)),
            );
        }
        Ok(())
    }
    .await?;

    Ok(())
}

// This is where send/recv streams from client are handled
// This function should get a server from the pool and start a connection
// with it. This should be done after initial message is received from the
// client stream. Make it into a cycle of receiving and sending information between
// the server and client then back to the server. This should end when the server ends
// its conection with the server.
async fn handle_response(
    (mut client_send, mut client_recv): (quinn::SendStream, quinn::RecvStream),
    mut server_conn: ServerConnect,
) -> Result<()> {
    println!("received new message");

    let (mut server_send, mut server_recv) = match server_conn.connect().await {
        Ok(s) => s,
        Err(_) => panic!("Cannot get streams for server connection"),
    };

    let mut incoming = bytes::BytesMut::new();
    let mut client_recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
    let mut client_msg_size = 0;
    let mut server_recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
    let mut server_msg_size = 0;

    while let Some(s) = client_recv
        .read(&mut client_recv_buffer)
        .await
        .map_err(|e| anyhow!("Could not read message from recv stream: {}", e))?
    {
        client_msg_size += s;
        incoming.extend_from_slice(&client_recv_buffer[0..s]);
        println!(
            "Received from Client stream: {}",
            std::str::from_utf8(&client_recv_buffer[0..s]).unwrap()
        );
    }
    println!("Received {} bytes from stream", client_msg_size);

    let body = "To Server".as_bytes();
    server_send.write_all(&body).await?;
    server_send.finish().await?;
    println!("Written to server");

    while let Some(s) = server_recv
        .read(&mut server_recv_buffer)
        .await
        .map_err(|e| anyhow!("Could not read message from recv stream: {}", e))?
    {
        server_msg_size += s;
        incoming.extend_from_slice(&server_recv_buffer[0..s]);
        println!(
            "Received from Server stream: {}",
            std::str::from_utf8(&server_recv_buffer[0..s]).unwrap()
        );
    }

    println!("writing message to send client stream...");
    client_send
        .write_all(&server_recv_buffer[0..server_msg_size])
        .await?;

    println!("closing send stream...");
    client_send.finish().await?;
    server_conn.close().await;

    println!("response handled!\n");
    Ok(())
}

fn func() {}
