use anyhow::{anyhow, Result};
use futures::{StreamExt, TryFutureExt};
use quinn::ServerConfig;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

#[allow(dead_code)]
fn main() {
    let exit_code = if let Err(e) = main_run(true) {
        eprintln!("ERROR: {}", e);
        1
    } else {
        0
    };

    std::process::exit(exit_code);
}

#[tokio::main]
pub async fn main_run(while_toggle: bool) -> Result<()> {
    run(while_toggle, 5347, 6347).await.unwrap();
    Ok(())
} 

pub async fn run(while_toggle: bool, quic: u16, hb: u16) -> Result<()> {

    let mut transport_config = quinn::TransportConfig::default();
    transport_config.stream_window_uni(0);
    transport_config.stream_window_bidi(10); // so it exhibits the problem quicker
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(1)));
    let mut quinn_config = quinn::ServerConfig::default();
    quinn_config.transport = Arc::new(transport_config);

    let mut server_config_builder = quinn::ServerConfigBuilder::new(quinn_config);
    server_config_builder.enable_keylog();
    server_config_builder.use_stateless_retry(true);
    server_config_builder.protocols(CUSTOM_PROTO); // custom protocol

    let key_path = std::path::PathBuf::from("signed.key");

    let key = std::fs::read(&key_path)
        .map_err(|e| anyhow!("(Server) Could not read cert key file from self_signed.key: {}", e))?;
    let key = quinn::PrivateKey::from_pem(&key)
        .map_err(|e| anyhow!("(Server) Could not create PEM from private key: {}", e))?;

    let cert_path = std::path::PathBuf::from("signed.pem");
    let cert_chain = std::fs::read(&cert_path)
        .map_err(|e| anyhow!("(Server) Could not read certificate chain file: {}", e))?;
    let cert_chain = quinn::CertificateChain::from_pem(&cert_chain)
        .map_err(|e| anyhow!("(Server) Could not create certificate chain: {}", e))?;

    server_config_builder.certificate(cert_chain, key)?;

    let server_config = server_config_builder.build();

    tokio::try_join!(build_and_run_server(quic, hb, server_config.clone(), while_toggle))?;

    println!("(Server) Shutting down...");

    Ok(())
}

pub async fn build_and_run_server(quic: u16, hb: u16, server_config: ServerConfig, while_toggle: bool) -> Result<()> {
    let mut endpoint_builder = quinn::Endpoint::builder();
    endpoint_builder.listen(server_config.clone());

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), quic);

    let mut incoming = {
        let (endpoint, incoming) = endpoint_builder.bind(&socket_addr)?;
        println!("(Server) Server listening on {}", endpoint.local_addr()?);
        incoming
    };

    tokio::spawn(async move {
        let home: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), hb);
        println!("(Server Health) Starting Server HB: {}", &home);
        let mut socket = UdpSocket::bind(home).await.unwrap();
        let mut buf = [0; 1];
        let mut to_send = None;

        loop {
            // First we check to see if there's a message we need to echo back.
            // If so then we try to send it back to the original source, waiting
            // until it's writable and we're able to do so.
            if let Some((_, peer)) = to_send {
                let amt = socket.send_to("a".as_bytes(), &peer).await.unwrap();

                println!("(Server Health) Sent {} to {}",amt, peer);
            }

            // If we're here then `to_send` is `None`, so we take a look for the
            // next message we're going to echo back.
            to_send = Some(socket.recv_from(&mut buf).await.unwrap());
        }
    });

    if while_toggle {
        while let Some(conn) = incoming.next().await {
            println!("(Server) {}: new connection!", socket_addr);
            tokio::spawn(handle_conn(conn).unwrap_or_else(move |e| {
                println!("(Server) {}: connection failed: {}", socket_addr, e);
            }));
        }

    }
    else {
        if let Some(conn) = incoming.next().await {
            println!("(Server) {}: new connection!", socket_addr);
            tokio::spawn(handle_conn(conn).unwrap_or_else(move |e| {
                println!("(Server) {}: connection failed: {}", socket_addr, e);
            }));
        }
    
        tokio::time::delay_for(Duration::new(2,0)).await;

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
                    .unwrap_or_else(move |e| eprintln!("(Server) Response failed: {}", e)),
            );
        }
        Ok(())
    }
    .await?;

    Ok(())
}

async fn handle_response(
    (mut send, mut recv): (quinn::SendStream, quinn::RecvStream),
) -> Result<()> {
    println!("(Server) Received new message");

    let mut incoming = bytes::BytesMut::new();
    let mut recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
    let mut msg_size = 0;

    while let Some(s) = recv
        .read(&mut recv_buffer)
        .await
        .map_err(|e| anyhow!("(Server) Could not read message from recv stream: {}", e))?
    {
        println!("(Server) Receiving data");
        msg_size += s;
        incoming.extend_from_slice(&recv_buffer[0..s]);
    }

    let msg_recv = std::str::from_utf8(&recv_buffer[0..msg_size]).unwrap();
    println!("(Server) Received {} bytes from stream: {}", msg_size, msg_recv);

    let body = "Returned".as_bytes();

    println!("(Server) Writing message to send stream...");
    send.write_all(&body).await?;

    println!("(Server) Closing send stream...");
    send.finish().await?;

    println!("(Server) Response handled!");
    Ok(())
}
