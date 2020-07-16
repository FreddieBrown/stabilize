use crate::backend::ServerConnect;
use crate::backend::ServerPool;
use crate::backend::Algo;
use anyhow::{anyhow, Result};
use futures::{StreamExt, TryFutureExt};
use quinn::ServerConfig;
use std::sync::Arc;
use std::{net::IpAddr, net::Ipv4Addr, net::SocketAddr};

/// Function will create an endpoint for clients to connect to and sets the port that
/// it will listen to. It will then listen for a new connection and will pass off to the
/// correct function when a connection occurs.
pub async fn build_and_run_server(port: u16, server_config: ServerConfig, config: &str) -> Result<()> {
    let mut endpoint_builder = quinn::Endpoint::builder();
    endpoint_builder.listen(server_config.clone());
    let serverpool = Arc::new(ServerPool::create_from_file(config, Algo::RoundRobin));

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let mut incoming = {
        let (endpoint, incoming) = endpoint_builder.bind(&socket_addr)?;
        println!("(Stabilize) Server listening on {}", endpoint.local_addr()?);
        incoming
    };

    // Create thread to start health checking on background servers
    let sp_health = serverpool.clone();
    tokio::spawn(async move {
        println!("(Stabilize Health) Starting Health Check");
        let home = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 4999);
        ServerPool::check_health_runner(sp_health, home, 5).await;
    });

    let serverpool_in = serverpool.clone();
    while let Some(conn) = incoming.next().await {
        println!("(Stabilize) {}: new connection!", socket_addr);
        let server = ServerPool::get_next(&serverpool_in).await;
        println!(
            "(Stabilize) Server given from server pool: {}",
            server.get_quic()
        );
        tokio::spawn(
            handle_conn(conn, server.get_quic(), serverpool_in.clone()).unwrap_or_else(move |e| {
                println!("(Stabilize) {}: connection failed: {}", socket_addr, e);
            }),
        );
    }

    Ok(())
}

/// Function will create an endpoint for clients to connect to and sets the port that
/// it will listen to. It will then listen for a new connection and will pass off to the
/// correct function when a connection occurs.
pub async fn build_and_run_test_server(port: u16, server_config: ServerConfig, config: &str) -> Result<()> {
    let mut endpoint_builder = quinn::Endpoint::builder();
    endpoint_builder.listen(server_config.clone());
    let serverpool = Arc::new(ServerPool::create_from_file(config, Algo::RoundRobin));

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let mut incoming = {
        let (endpoint, incoming) = endpoint_builder.bind(&socket_addr)?;
        println!("(Stabilize) Server listening on {}", endpoint.local_addr()?);
        incoming
    };

    // Create thread to start health checking on background servers
    let sp_health = serverpool.clone();
    tokio::spawn(async move {
        println!("(Stabilize Health) Starting Health Check");
        let home = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 4999);
        ServerPool::check_health_runner(sp_health, home, 5).await;
    });

    let serverpool_in = serverpool.clone();
    let mut count = 0;
    while count != 1 {
        let conn = incoming.next().await.unwrap();
        println!("(Stabilize) {}: new connection!", socket_addr);
        let server = ServerPool::get_next(&serverpool_in).await;
        println!(
            "(Stabilize) Server given from server pool: {}",
            server.get_quic()
        );
        handle_conn(conn, server.get_quic(), serverpool_in.clone())
            .unwrap_or_else(move |e| {
                println!("(Stabilize) {}: connection failed: {}", socket_addr, e)
            })
            .await;
        count += 1;
    }

    Ok(())
}

/// This function will handle any incoming connections. It will start a connection with
/// the Server that it is going to connects to and will pass off to handle_response when a
/// message has been received from the client.
async fn handle_conn(conn: quinn::Connecting, server: SocketAddr, serverpool: Arc<ServerPool>) -> Result<()> {
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
    Algo::decrement_connections(&serverpool, server).await;
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
    println!("(Stabilize) Received {} bytes from Client stream", client_msg_size);

    match filter(&client_recv_buffer[0..client_msg_size], Filter::NoFilter) {
        Some(_) => {
            println!("Message passed through Client filtering");
        },
        None => {
            server_send.finish().await?;
            client_send.finish().await?;
            server_conn.close().await;
            return Ok(());
        }
    };

    server_send
        .write_all(&client_recv_buffer[0..client_msg_size])
        .await?;
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

    match filter(&server_recv_buffer[0..server_msg_size], Filter::NoFilter) {
        Some(_) => {
            println!("Message passed through Server filtering");
        },
        None => {
            server_send.finish().await?;
            client_send.finish().await?;
            server_conn.close().await;
            return Ok(());
        }
    };

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

/// Packet filtring architecture. Enums to decide what type of filtering 
/// is done by the load balancer.
fn filter(buffer: &[u8], filter: Filter) -> Option<&[u8]>{
    match filter{
        Filter::NoFilter => Filter::no_filter(buffer),
        Filter::AllFilter => Filter::all_filter(buffer)
    }
}

pub enum Filter {
    NoFilter,
    AllFilter
}

#[allow(unused_variables)]
impl Filter {
    /// No filtering function. Packet will be passed straight through
    fn no_filter(buffer: &[u8]) -> Option<&[u8]> {
        Some(buffer)
    }

    /// Filters out all traffic
    fn all_filter(buffer: &[u8]) -> Option<&[u8]>{
        None
    }
}


#[cfg(test)]
mod tests;