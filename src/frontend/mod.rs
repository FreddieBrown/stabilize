use std::{net::IpAddr, net::Ipv4Addr, net::SocketAddr};
use std::sync::Arc;
use crate::backend::ServerConnect;
use crate::backend::ServerPool;
use anyhow::{anyhow, Result};
use futures::{StreamExt, TryFutureExt};
use quinn::ServerConfig;

/// Function will create an endpoint for clients to connect to and sets the port that
/// it will listen to. It will then listen for a new connection and will pass off to the
/// correct function when a connection occurs.
pub async fn build_and_run_server(port: u16, server_config: ServerConfig) -> Result<()> {
    let mut endpoint_builder = quinn::Endpoint::builder();
    endpoint_builder.listen(server_config.clone());
    let serverpool = Arc::new(ServerPool::create_from_file("./.config.toml"));

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let mut incoming = {
        let (endpoint, incoming) = endpoint_builder.bind(&socket_addr)?;
        println!("(Stabilize) Server listening on {}", endpoint.local_addr()?);
        incoming
    };
    let serverpool_in = serverpool.clone();
    while let Some(conn) = incoming.next().await{
        println!("(Stabilize) {}: new connection!", socket_addr);
        let server = serverpool_in.get_next().await;
        println!(
            "(Stabilize) Server given from server pool: {}",
            server.get_quic()
        );
        tokio::spawn(
            handle_conn(conn, server.get_quic()).unwrap_or_else(move |e| {
                println!("(Stabilize) {}: connection failed: {}", socket_addr, e);
            }),
        );
 
    }

    Ok(())
}

/// Function will create an endpoint for clients to connect to and sets the port that
/// it will listen to. It will then listen for a new connection and will pass off to the
/// correct function when a connection occurs.
pub async fn build_and_run_test_server(port: u16, server_config: ServerConfig) -> Result<()> {
    let mut endpoint_builder = quinn::Endpoint::builder();
    endpoint_builder.listen(server_config.clone());
    let serverpool = Arc::new(ServerPool::create_from_file("./.config.toml"));

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let mut incoming = {
        let (endpoint, incoming) = endpoint_builder.bind(&socket_addr)?;
        println!("(Stabilize) Server listening on {}", endpoint.local_addr()?);
        incoming
    };
    let serverpool_in = serverpool.clone();
    let mut count = 0;
    while count != 1{
        let conn = incoming.next().await.unwrap();
        println!("(Stabilize) {}: new connection!", socket_addr);
        let server = serverpool_in.get_next().await;
        println!(
            "(Stabilize) Server given from server pool: {}",
            server.get_quic()
        );
        handle_conn(conn, server.get_quic()).unwrap_or_else(move |e| {
            println!("(Stabilize) {}: connection failed: {}", socket_addr, e)}).await;
        count += 1;
 
    }

    Ok(())
}

/// This function will handle any incoming connections. It will start a connection with
/// the Server that it is going to connects to and will pass off to handle_response when a
/// message has been received from the client.
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
            let server_conn = match ServerConnect::start(&server).await {
                Ok(conn) => conn,
                Err(_) => panic!("(Stabilize) Server isn't alive"),
            };
            // Spawn message handling task
            tokio::spawn(
                handle_response(client_streams, server_conn)
                    .unwrap_or_else(move |e| eprintln!("(Stabilize) Response failed: {}", e)),
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
    println!("(Stabilize) Received new message");

    let (mut server_send, mut server_recv) = match server_conn.connect().await {
        Ok(s) => s,
        Err(_) => panic!("(Stabilize) Cannot get streams for server connection"),
    };

    let mut incoming = bytes::BytesMut::new();
    let mut client_recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
    let mut client_msg_size = 0;
    let mut server_recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
    let mut server_msg_size = 0;

    while let Some(s) = client_recv
        .read(&mut client_recv_buffer)
        .await
        .map_err(|e| anyhow!("(Stabilize) Could not read message from recv stream: {}", e))?
    {
        client_msg_size += s;
        incoming.extend_from_slice(&client_recv_buffer[0..s]);
        println!(
            "(Stabilize) Received from Client stream: {}",
            std::str::from_utf8(&client_recv_buffer[0..s]).unwrap()
        );
    }
    println!("(Stabilize) Received {} bytes from stream", client_msg_size);

    server_send.write_all(&client_recv_buffer[0..client_msg_size]).await?;
    server_send.finish().await?;
    println!("(Stabilize) Written to server");

    while let Some(s) = server_recv
        .read(&mut server_recv_buffer)
        .await
        .map_err(|e| anyhow!("(Stabilize) Could not read message from recv stream: {}", e))?
    {
        server_msg_size += s;
        incoming.extend_from_slice(&server_recv_buffer[0..s]);
        println!(
            "(Stabilize) Received from Server stream: {}",
            std::str::from_utf8(&server_recv_buffer[0..s]).unwrap()
        );
    }

    println!("(Stabilize) Writing message to send client stream...");
    client_send
        .write_all(&server_recv_buffer[0..server_msg_size])
        .await?;

    println!("(Stabilize) Closing send stream...");
    client_send.finish().await?;
    server_conn.close().await;

    println!("(Stabilize) Response handled!\n");
    Ok(())
}