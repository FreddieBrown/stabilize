#![warn(rust_2018_idioms)]

use std::{
    ascii, fs, io,
    net::SocketAddr,
    path::{self, Path, PathBuf},
    str,
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};
use futures::{StreamExt, TryFutureExt};
use structopt::{self, StructOpt};
use tracing::{error, info, info_span};
use tracing_futures::Instrument as _;


// From Quinn example
#[derive(StructOpt, Debug)]
#[structopt(name = "server")]
struct Opt {
    /// file to log TLS keys to for debugging
    #[structopt(long = "keylog")]
    keylog: bool,
    /// directory to serve files from
    #[structopt(parse(from_os_str))]
    root: PathBuf,
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
    #[structopt(long = "listen", default_value = "[::1]:4433")]
    listen: SocketAddr,
}

fn main() {

    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .finish(),
    )
    .unwrap();
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
async fn run(options: Opt) -> Result<()>{

    Ok(())

}
