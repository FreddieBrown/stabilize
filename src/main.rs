#![warn(rust_2018_idioms)]

use std::{
    path::PathBuf,
    sync::Arc,
};

use structopt::{self, StructOpt};
use anyhow::{anyhow, Result};
mod frontend;
mod backend;



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

    tokio::try_join!(frontend::build_and_run_server(opt.listen, server_config.clone()))?;

    println!("shutting down...");

    Ok(())
}


mod tests;