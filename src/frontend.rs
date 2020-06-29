use std::{
    net::SocketAddr,
    net::IpAddr, net::Ipv4Addr,
};

use futures::{StreamExt, TryFutureExt};
use anyhow::{anyhow, Result};
use quinn::ServerConfig;



pub async fn build_and_run_server(port: u16, server_config: ServerConfig) -> Result<()> {
    let mut endpoint_builder = quinn::Endpoint::builder();
    endpoint_builder.listen(server_config.clone());

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let mut incoming = {
        let (endpoint, incoming) = endpoint_builder.bind(&socket_addr)?;
        println!("Server listening on {}", endpoint.local_addr()?);
        incoming
    };

    while let Some(conn) = incoming.next().await {
        println!("{}: new connection!", socket_addr);
        tokio::spawn(handle_conn(conn).unwrap_or_else(move |e| {
            println!("{}: connection failed: {}", socket_addr, e);
        }));
    }

    Ok(())
}

async fn handle_conn(conn: quinn::Connecting) -> Result<()> {
    let quinn::NewConnection {
        connection: _connection,
        mut bi_streams,
        ..
    } = conn.await?;

    // dispatch the actual handling as another future. concurrency!
    async {
        // each stream needs to be processed independently
        while let Some(stream) = bi_streams.next().await {
            let send_recv = match stream {
                Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                    // application closed, finish up early
                    return Ok(());
                }
                Err(e) => {
                    return Err(e);
                }
                Ok(s) => s,
            };
            tokio::spawn(
                handle_response(send_recv)
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
    (mut send, mut recv): (quinn::SendStream, quinn::RecvStream),
) -> Result<()> {
    println!("received new message");

    let mut incoming = bytes::BytesMut::new();
    let mut recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
    let mut msg_size = 0;

    while let Some(s) = recv
        .read(&mut recv_buffer)
        .await
        .map_err(|e| anyhow!("Could not read message from recv stream: {}", e))?
    {
        msg_size += s;
        incoming.extend_from_slice(&recv_buffer[0..s]);
    }
    println!("Received {} bytes from stream", msg_size);

    let body = tokio::task::spawn_blocking(|| -> Result<Vec<u8>> {
        Ok(std::fs::read("./hi.txt")?)
    }).await??;

    println!("writing message to send stream...");
    send.write_all(&body).await?;

    println!("closing send stream...");
    send.finish().await?;

    println!("response handled!");
    Ok(())
}