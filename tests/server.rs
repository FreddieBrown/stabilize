use anyhow::{anyhow, Result};
use futures::{StreamExt, TryFutureExt};
use quinn::ServerConfig;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use std::path::PathBuf;
use structopt::{self, StructOpt};

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "stabilize-server")]
pub struct Opt {
    /// Quic port to use for connections
    #[structopt(long = "quic", short = "q", default_value = "5347")]
    quic: u16,
    /// Port to use to send hearbeat over
    #[structopt(long = "heartbeat", short = "hb", default_value="6347")]
    hb: u16 
}

#[allow(dead_code)]
fn main() {
    let opt = Opt::from_args();
    let code = {
        if let Err(e) = main_run(true, opt) {
            log::error!("ERROR: {}", e);
            1
        } else {
            0
        }
    };
    std::process::exit(code);
}

#[tokio::main]
pub async fn main_run(while_toggle: bool, opt: Opt) -> Result<()> {
    run(while_toggle, opt.quic, opt.hb).await.unwrap();
    Ok(())
} 

pub async fn run(while_toggle: bool, quic: u16, hb: u16) -> Result<()> {

    let server_config = config_builder().await?;

    tokio::try_join!(build_and_run_server(quic, hb, server_config.clone(), while_toggle))?;

    println!("(Server) Shutting down...");

    Ok(())
}

pub async fn config_builder() -> Result<quinn::ServerConfig> {
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

    let key_path = PathBuf::from("key.der");
    let cert_path = PathBuf::from("cert.der");
    let (cert, key) = match std::fs::read(&cert_path).and_then(|x| Ok((x, std::fs::read(&key_path)?))) {
        Ok(x) => x,
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let key = cert.serialize_private_key_der();
            let cert = cert.serialize_der().unwrap();
            std::fs::write(&cert_path, &cert)?;
            std::fs::write(&key_path, &key)?;
            (cert, key)
        }
        Err(e) => {
            panic!("failed to read certificate: {}", e);
        }
    };
    let key = quinn::PrivateKey::from_der(&key)?;
    let cert = quinn::Certificate::from_der(&cert)?;
    server_config_builder.certificate(quinn::CertificateChain::from_certs(vec![cert]), key).unwrap();
    Ok(server_config_builder.build())
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
