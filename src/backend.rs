use std::{
    fs::File,
    io::{prelude::*, BufReader},
    net::SocketAddr,
    sync::Arc,
};

use anyhow::{Context, Result};
use rustls;
use tokio::sync::RwLock;

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

pub struct ServerInfo {
    alive: bool,
}

impl ServerInfo {
    pub fn new() -> ServerInfo {
        ServerInfo { alive: true }
    }
}

pub struct Server {
    addr: SocketAddr,
}

impl Server {
    pub fn new(addr: SocketAddr) -> Server {
        Server { addr }
    }

    pub fn get_addr(&self) -> SocketAddr {
        self.addr.clone()
    }
}

pub struct ServerPool {
    servers: Vec<(Server, RwLock<ServerInfo>)>,
    current: RwLock<usize>,
}

impl ServerPool {
    pub fn create_from_file() -> ServerPool {
        // Open up file from config path
        // Go through the config and create a HashMap which contains Server structs
        // based on the addresses in the config file
        let mut list = Vec::new();
        let file = File::open("./.config").unwrap();
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let addr = line.unwrap();
            println!("{}", &addr);
            let addr = addr.parse::<SocketAddr>().unwrap();
            list.push((Server::new(addr), RwLock::new(ServerInfo::new())));
        }
        ServerPool {
            servers: list,
            current: RwLock::new(0),
        }
    }

    pub fn new() -> ServerPool {
        ServerPool {
            servers: Vec::new(),
            current: RwLock::new(0),
        }
    }

    pub fn add(&mut self, server: Server) {
        self.servers.push((server, RwLock::new(ServerInfo::new())));
    }

    pub async fn get_next(&self) -> &Server {
        let mut r_curr = self.current.write().await;
        println!("Getting a server");

        loop {
            let (server, server_info) = &self.servers[*r_curr];
            let server_info = server_info.read().await;
            if !server_info.alive {
                println!("Server is not alive: {}", server.get_addr());
                *r_curr += 1;

                if *r_curr == self.servers.len() {
                    *r_curr = 0;
                }
            } else {
                *r_curr += 1;
                return server;
            }
        }
    }
    // Write health checking functions
}

pub struct ServerConnect {
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
}

impl ServerConnect {
    pub async fn connect(&mut self) -> Result<(quinn::SendStream, quinn::RecvStream)> {
        Ok(self.connection.open_bi().await?)
    }

    pub async fn close(&self) {
        self.endpoint.close(0u8.into(), b"done");
    }

    pub async fn start(addr: &SocketAddr) -> Result<ServerConnect> {
        let mut crypto = rustls::ClientConfig::new();
        crypto.versions = vec![rustls::ProtocolVersion::TLSv1_3];

        // Change this to make it more secure
        crypto
            .dangerous()
            .set_certificate_verifier(Arc::new(insecure::NoCertificateVerification {}));

        let config = quinn::ClientConfig {
            transport: Arc::new(quinn::TransportConfig::default()),
            crypto: Arc::new(crypto),
        };
        let mut client_config = quinn::ClientConfigBuilder::new(config);
        client_config.protocols(CUSTOM_PROTO);
        let (endpoint, _) = quinn::Endpoint::builder()
            .bind(&"[::]:0".parse().unwrap())
            .context("Could not bind client endpoint")?;
        let conn = endpoint
            .connect_with(client_config.build(), addr, "localhost")?
            .await
            .context(format!("Could not connect to {}", addr))?;
        let quinn::NewConnection {
            connection: conn, ..
        } = { conn };

        Ok(ServerConnect {
            endpoint,
            connection: conn,
        })
    }
}

mod insecure {
    use rustls;
    use webpki;

    pub struct NoCertificateVerification {}

    impl rustls::ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _roots: &rustls::RootCertStore,
            _presented_certs: &[rustls::Certificate],
            _dns_name: webpki::DNSNameRef<'_>,
            _ocsp: &[u8],
        ) -> Result<rustls::ServerCertVerified, rustls::TLSError> {
            Ok(rustls::ServerCertVerified::assertion())
        }
    }
}
