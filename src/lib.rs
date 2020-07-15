use std::{path::PathBuf, sync::Arc, error::Error};

use anyhow::{anyhow, Result};
use structopt::{self, StructOpt};

pub mod backend;
pub mod frontend;

// From Quinn example
#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "stabilize")]
pub struct Opt {
    /// file to log TLS keys to for debugging
    #[structopt(long = "keylog")]
    keylog: bool,
    /// Enable stateless retries
    #[structopt(long = "stateless-retry")]
    stateless_retry: bool,
    /// Address to listen on
    #[structopt(long = "listen", default_value = "4433")]
    listen: u16,
}

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

#[tokio::main]
pub async fn run(opt: Opt) -> Result<()> {
    tracing_subscriber::fmt::init();

    let server_config = config_builder().await?;

    tokio::try_join!(frontend::build_and_run_server(
        opt.listen,
        server_config.clone(),
        "./.config.toml"
    ))?;

    println!("(Stabilize) shutting down...");

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
