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

/// Function to encapsulate the state of a server
pub struct ServerInfo {
    alive: bool,
}

impl ServerInfo {
    /// Will create a new ServerInfo object.
    pub fn new() -> ServerInfo {
        ServerInfo { alive: true }
    }
}

/// Server object. Contains server address but in the
/// future will contain more extensive information about
/// the servers.
pub struct Server {
    addr: SocketAddr,
}

/// Functions to help the server to work. Function to build a new
/// server object and one to return the address.
impl Server {
    pub fn new(addr: SocketAddr) -> Server {
        Server { addr }
    }

    pub fn get_addr(&self) -> SocketAddr {
        self.addr.clone()
    }
}

/// ServerPool struct contains a list of servers and data about them,
/// as well as the RoundRobin counter for selecting a server.
pub struct ServerPool {
    servers: Vec<(Server, RwLock<ServerInfo>)>,
    current: RwLock<usize>,
}

/// ServerPool functions
impl ServerPool {
    /// This function will go through a config file which contains the
    /// servers it needs to connect to and will build a ServerPool instance
    /// using these server addresses
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

    /// Create a new, blank ServerPool object
    pub fn new() -> ServerPool {
        ServerPool {
            servers: Vec::new(),
            current: RwLock::new(0),
        }
    }

    /// Add a Server to the ServerPool
    pub fn add(&mut self, server: Server) {
        self.servers.push((server, RwLock::new(ServerInfo::new())));
    }

    /// Function to get the next Server from the ServerPool.
    /// This is the function which implements the LB algorithms.
    /// In future, this should be abstracted to allow other LB
    /// algos to be used to allow variety in the types of load balancer
    /// algorithms that can be used.
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

/// ServerConnect object is used during connection with a server. Holds
/// both the endpoint of the connection as well as the connection itself.
pub struct ServerConnect {
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
}

impl ServerConnect {
    /// Function to fully connect Stabilize to Server
    pub async fn connect(&mut self) -> Result<(quinn::SendStream, quinn::RecvStream)> {
        Ok(self.connection.open_bi().await?)
    }

    /// Closes endpoint for server connection
    pub async fn close(&self) {
        self.endpoint.close(0u8.into(), b"done");
    }

    /// Creates a ServerConnect object and configures a connection between Stabilize and
    /// another Quic server.
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

/// Module that allows for insurce connection. Change in future to
/// enforce secure connections.
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
