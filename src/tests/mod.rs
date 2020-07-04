use crate::backend::ServerConnect;
use crate::backend::ServerPool;
use anyhow::{anyhow, Result};
use tokio_test;
use std::sync::Arc;
use quinn::ServerConfig;
use futures::{StreamExt, TryFutureExt};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
mod client;
mod server;
#[test]
fn test_sanity() {
    assert_eq!(1, 1);
}

#[tokio::test]
async fn test_stab_to_server() -> Result<()> {
    println!("About to run server socket");

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
    server_config_builder.protocols(server::CUSTOM_PROTO); // custom protocol

    let key_path = std::path::PathBuf::from("signed.key");

    let key = std::fs::read(&key_path)
        .map_err(|e| anyhow!("Could not read cert key file from self_signed.key: {}", e))?;
    let key = quinn::PrivateKey::from_pem(&key)
        .map_err(|e| anyhow!("Could not create PEM from private key: {}", e))?;

    let cert_path = std::path::PathBuf::from("signed.pem");
    let cert_chain = std::fs::read(&cert_path)
        .map_err(|e| anyhow!("Could not read certificate chain file: {}", e))?;
    let cert_chain = quinn::CertificateChain::from_pem(&cert_chain)
        .map_err(|e| anyhow!("Could not create certificate chain: {}", e))?;

    server_config_builder.certificate(cert_chain, key)?;

    let server_config = server_config_builder.build();

    tokio::spawn(async move {
        println!("About to run client socket");
        let mut incoming = bytes::BytesMut::new();
        let serverpool = ServerPool::create_from_file();
        let server = serverpool.get_next().await;
        println!("Server given from server pool: {}", server.get_addr());
        tokio::time::delay_for(Duration::new(1,0)).await;
        let mut server_conn = match ServerConnect::start(&server.get_addr()).await {
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
        send.write_all(&body).await;
        send.finish().await.unwrap();
        println!("Written to Server");

        while let Some(s) = recv
            .read(&mut recv_buffer)
            .await
            .map_err(|e| anyhow!("Could not read message from recv stream: {}", e)).unwrap()
        {
            msg_size += s;
            incoming.extend_from_slice(&recv_buffer[0..s]);
            println!(
                "Received from stream: {}",
                std::str::from_utf8(&recv_buffer[0..s]).unwrap()
            );  
        }
        assert_eq!("Returned", std::str::from_utf8(&recv_buffer[0..msg_size]).unwrap());
    });

    tokio::try_join!(server::build_and_run_server(
        5347,
        server_config.clone(),
        false
    ))?;

    println!("shutting down...");
    assert_eq!(1, 1);
    Ok(())

}

#[test]
fn test_client_to_server() {
    
}

#[test]
fn test_server_healthcheck() {
    assert_eq!(1, 1);
}
