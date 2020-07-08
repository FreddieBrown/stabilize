use std::{path::PathBuf, sync::Arc};

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

#[tokio::main]
pub async fn run(opt: Opt) -> Result<()> {
    tracing_subscriber::fmt::init();

    let server_config = config_builder(opt.clone()).await?;

    tokio::try_join!(frontend::build_and_run_server(
        opt.listen,
        server_config.clone(),
        "./.config.toml"
    ))?;

    println!("(Stabilize) shutting down...");

    Ok(())
}

async fn config_builder(opt: Opt) -> Result<quinn::ServerConfig> {
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

    let key = std::fs::read(&opt.key.unwrap()).map_err(|e| {
        anyhow!(
            "(Stabilize) Could not read cert key file from self_signed.key: {}",
            e
        )
    })?;
    let key = quinn::PrivateKey::from_pem(&key)
        .map_err(|e| anyhow!("(Stabilize) Could not create PEM from private key: {}", e))?;

    let cert_chain = std::fs::read(&opt.cert.unwrap())
        .map_err(|e| anyhow!("(Stabilize) Could not read certificate chain file: {}", e))?;
    let cert_chain = quinn::CertificateChain::from_pem(&cert_chain)
        .map_err(|e| anyhow!("(Stabilize) Could not create certificate chain: {}", e))?;

    server_config_builder.certificate(cert_chain, key).unwrap();
    Ok(server_config_builder.build())
}

pub async fn config_builder_raw(
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
    stateless_retry: bool,
) -> Result<quinn::ServerConfig> {
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.stream_window_uni(0);
    transport_config.stream_window_bidi(10); // so it exhibits the problem quicker
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(1)));
    let mut quinn_config = quinn::ServerConfig::default();
    quinn_config.transport = Arc::new(transport_config);

    let mut server_config_builder = quinn::ServerConfigBuilder::new(quinn_config);
    server_config_builder.enable_keylog();
    server_config_builder.use_stateless_retry(stateless_retry);
    server_config_builder.protocols(CUSTOM_PROTO); // custom protocol

    let key = std::fs::read(&key.unwrap()).map_err(|e| {
        anyhow!(
            "(Stabilize) Could not read cert key file from self_signed.key: {}",
            e
        )
    })?;
    let key = quinn::PrivateKey::from_pem(&key)
        .map_err(|e| anyhow!("(Stabilize) Could not create PEM from private key: {}", e))?;

    let cert_chain = std::fs::read(&cert.unwrap())
        .map_err(|e| anyhow!("(Stabilize) Could not read certificate chain file: {}", e))?;
    let cert_chain = quinn::CertificateChain::from_pem(&cert_chain)
        .map_err(|e| anyhow!("(Stabilize) Could not create certificate chain: {}", e))?;

    server_config_builder.certificate(cert_chain, key).unwrap();
    Ok(server_config_builder.build())
}
