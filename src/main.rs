#![warn(rust_2018_idioms)]

use std::{
    ascii, fs, io,
    net::SocketAddr,
    path::{self, Path, PathBuf},
    str,
    sync::Arc,
    net::IpAddr, net::Ipv4Addr,
};

use futures::{StreamExt, TryFutureExt};
use structopt::{self, StructOpt};
use anyhow::{anyhow, Result};
use quinn::ServerConfig;


#[allow(unused)]
// From Quinn example
#[derive(StructOpt, Debug)]
#[structopt(name = "server")]
struct Opt {
    /// file to log TLS keys to for debugging
    #[structopt(long = "keylog")]
    keylog: bool,
    /// TLS private key in PEM format
    #[structopt(parse(from_os_str), short = "k", long = "key", requires = "cert")]
    key: Option<PathBuf>,
    /// TLS certificate in PEM format
    #[structopt(parse(from_os_str), short = "c", long = "cert", requires = "key")]
    cert: Option<PathBuf>,
    /// Enable stateless retries
    #[structopt(long = "stateless-retry")]
    stateless_retry: bool,
    /// Address to listen on
    #[structopt(long = "listen", default_value = "4433")]
    listen: u16,
}

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

fn main() {
    let opt = Opt::from_args();
    let code = {
        if let Err(e) = run(opt) {
            eprintln!("ERROR: {}", e);
            1
        } else {
            0
        }
    };
    println!("Hello World!");
    ::std::process::exit(code);
}

#[tokio::main]
async fn run(opt: Opt) -> Result<()> {
    tracing_subscriber::fmt::init();
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

    // let key_path = std::path::PathBuf::from("self_signed.key");

    let key = std::fs::read(&opt.key.unwrap())
        .map_err(|e| anyhow!("Could not read cert key file from self_signed.key: {}", e))?;
    let key = quinn::PrivateKey::from_pem(&key)
        .map_err(|e| anyhow!("Could not create PEM from private key: {}", e))?;


    // let cert_path = std::path::PathBuf::from("self_signed.pem");
    let cert_chain = std::fs::read(&opt.cert.unwrap())
        .map_err(|e| anyhow!("Could not read certificate chain file: {}", e))?;
    let cert_chain = quinn::CertificateChain::from_pem(&cert_chain)
        .map_err(|e| anyhow!("Could not create certificate chain: {}", e))?;


    server_config_builder.certificate(cert_chain, key)?;

    let server_config = server_config_builder.build();

    tokio::try_join!(build_and_run_server(opt.listen, server_config.clone()))?;

    println!("shutting down...");

    Ok(())
}

async fn build_and_run_server(port: u16, server_config: ServerConfig) -> Result<()> {
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
