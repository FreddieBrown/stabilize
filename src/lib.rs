use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use structopt::{self, StructOpt};

pub mod backend;
pub mod frontend;

use backend::Algo;

// From Quinn example
#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "stabilize")]
pub struct Opt {
    /// Address to listen on
    #[structopt(long = "listen", default_value = "4433")]
    listen: u16,
    /// Certificate path
    #[structopt(long = "cert", short = "c", parse(from_os_str))]
    cert: Option<PathBuf>,
    /// Key path
    #[structopt(long = "key", short = "k", parse(from_os_str))]
    key: Option<PathBuf>,
    /// Specify Protocol being used by stabilize
    #[structopt(long = "protocol", short = "p", default_value = "cstm-01")]
    protocol: String,
    /// LB Algo to use
    #[structopt(long="algo")]
    algo: Option<String>,
    /// Sticky Sessions switch
    #[structopt(long="sticky", short="s")]
    sticky: bool
}

#[tokio::main]
pub async fn run(opt: Opt) -> Result<()> {
    let algo = match &opt.algo {
        Some(algo) => match &algo[..] {
            "wrr" => Algo::WeightedRoundRobin,
            "lc" => Algo::LeastConnections,
            "cpu" => Algo::CpUtilise,
            _ => Algo::RoundRobin
        },
    _ => Algo::RoundRobin
    };

    let server_config = config_builder(
        opt.cert.clone(),
        opt.key.clone(),
        &[opt.protocol.as_bytes()],
    )
    .await?;
    tokio::try_join!(frontend::build_and_run_server(
        opt.clone(),
        server_config.clone(),
        "./.config.toml",
        algo,
        opt.sticky
    ))?;

    log::info!("(Stabilize) shutting down...");

    Ok(())
}

pub async fn config_builder(
    cert_opt: Option<PathBuf>,
    key_opt: Option<PathBuf>,
    protocol: &[&[u8]],
) -> Result<quinn::ServerConfig> {
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.stream_window_uni(0);
    transport_config.stream_window_bidi(10); // so it exhibits the problem quicker
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(1)));
    let mut quinn_config = quinn::ServerConfig::default();
    quinn_config.transport = Arc::new(transport_config);

    let mut server_config_builder = quinn::ServerConfigBuilder::new(quinn_config);
    server_config_builder.enable_keylog();
    server_config_builder.use_stateless_retry(true);
    server_config_builder.protocols(protocol); // custom protocol
    let (cert_path, key_path) = match (cert_opt, key_opt) {
        (Some(c), Some(k)) => (c, k),
        (_, _) => (PathBuf::from("cert.der"), PathBuf::from("key.der")),
    };
    let (cert, key) =
        match std::fs::read(&cert_path).and_then(|x| Ok((x, std::fs::read(&key_path)?))) {
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
    server_config_builder
        .certificate(quinn::CertificateChain::from_certs(vec![cert]), key)
        .unwrap();
    Ok(server_config_builder.build())
}
