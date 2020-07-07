use std::{
    fs::File,
    io::{prelude::*, BufReader},
    net::SocketAddr,
    sync::Arc,
};

use anyhow::{Context, Result};
use rustls;
use tokio::sync::RwLock;
use std::time::Duration;
use tokio::net::UdpSocket;
use serde::{Serialize, Deserialize};
use toml::Value;

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

#[derive(Deserialize, Debug)]
pub struct Config {
    servers: Vec<Server>,
}

impl Config {
    pub fn new() -> Config{
        Config{
            servers: vec![]
        }
    }
}

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
#[derive(Deserialize, Debug, Copy, Clone)]
pub struct Server {
    quic: SocketAddr,
    heartbeat: SocketAddr
}

/// Functions to help the server to work. Function to build a new
/// server object and one to return the address.
impl Server {
    pub fn new(quic: SocketAddr, heartbeat: SocketAddr) -> Server {
        Server { quic, heartbeat}
    }

    pub fn get_quic(&self) -> SocketAddr {
        self.quic.clone()
    }
    pub fn get_hb(&self) -> SocketAddr {
        self.heartbeat.clone()
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
    pub fn create_from_file(config: &str) -> ServerPool {
        // Open up file from config path
        // Go through the config and create a HashMap which contains Server structs
        // based on the addresses in the config file
        let mut config_toml = String::from("");
        let mut file = match File::open(&config) {
            Ok(file) => file,
            Err(_)  => {
                panic!("(Stabilize) Could not find config file, using default!");
            }
        };
    
        file.read_to_string(&mut config_toml)
                .unwrap_or_else(|err| panic!("(Stabilize) Error while reading config: [{}]", err));

        let config: Config = toml::from_str(&config_toml).unwrap();

        let servers = config.servers;

        println!("{:?}", servers);

        let list: Vec<_> = servers.iter().map(|s| (s.clone(), RwLock::new(ServerInfo::new())) ).collect();
            
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
        println!("(Stabilize) Getting a server");

        loop {
            let (server, server_info) = &self.servers[*r_curr];
            let server_info = server_info.read().await;
            if !server_info.alive {
                println!("(Stabilize) Server is not alive: {}", server.get_quic());
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

    /// Function to check if a server is alive at the specified addr port. It will send a short 
    /// message to the port and will wait for a response. If there is no response, it will assume 
    /// the server is dead and will move on.
    pub async fn heartbeat (addr: SocketAddr, home: SocketAddr) -> bool{
        let mut sock = UdpSocket::bind(home).await.expect(&format!("(Stabilize Health) Couldn't bind socket to address {}", addr));
        match sock.connect(addr).await {
            Ok(_) => println!("(Stabilize Health) Connected to address: {}", addr),
            Err(_) => println!("(Stabilize Health) Did not connect to address: {}", addr)
        };
        sock.send("a".as_bytes()).await.unwrap();
        let mut buf = [0; 1];
        match sock.recv(&mut buf).await {
            Ok(_) => {println!("(Stabilize Health) Received: {:?}, Server Alive {}", &buf, &addr); true},
            Err(_) => {println!("(Stabilize Health) Server dead: {}", &addr); false}
        }
    }

    /// Function to update a specified server info struct with information about that server
    pub async fn update_server_info(server: &Server, home: SocketAddr, info: &mut ServerInfo){
        println!("(Stabilize Health) Changing the status of {}", server.get_quic());
        info.alive = ServerPool::heartbeat(server.get_hb(), home).await;
        println!("(Stabilize Health) Alive: {}", info.alive);
    }

    /// This function will go through a serverpool and check the health of each server
    pub async fn check_health(serverpool: Arc<ServerPool>, home: SocketAddr) {
        println!("(Stabilize Health) This function will start to check the health of servers in the server pool");
        // Loop through all servers in serverpool
        for (server, servinfo) in &serverpool.servers{
            // Run check on each server
            println!("(Stabilize Health) Quic: {}, HB: {}",&server.quic, &server.heartbeat);
            let mut status;
            {
                let read = servinfo.read().await;
                status = read.alive;
            } 
            let mut temp = ServerInfo::new();
            ServerPool::update_server_info(&server, home, &mut temp).await;
            // If it has changed, then update the server status
            if !(status && temp.alive) {
                let mut write = servinfo.write().await;
                *write = temp;
            }
            // Otherwise, move onto next server
        }

    }

    /// This function will run the health checking functionality in a loop. Each time it is complete, 
    /// time will be taken for the function to rest before checking health again. This should be run 
    /// on a thread which lasts the length of the program.
    pub async fn check_health_runner(serverpool: Arc<ServerPool>, home: SocketAddr, delay: u64){
        // Abstract the number of seconds out to another place
        let mut interval = tokio::time::interval(Duration::from_secs(delay));

        loop {
            let sp = serverpool.clone();
            ServerPool::check_health(sp, home).await;
            interval.tick().await;
        }

    }
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
            .context("(Stabilize) Could not bind client endpoint")?;
        let conn = endpoint
            .connect_with(client_config.build(), addr, "localhost")?
            .await
            .context(format!("(Stabilize) Could not connect to {}", addr))?;
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

#[cfg(test)]
mod tests;