use std::{error::Error, fs::File, io::prelude::*, net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

#[derive(Deserialize, Debug)]
pub struct Config {
    servers: Vec<Server>,
}

impl Config {
    pub fn new() -> Config {
        Config { servers: vec![] }
    }
}

pub enum Algo {
    RoundRobin,
    LeastConnections,
}

impl Algo {
    /// Load Balancing algorithm. Implementation of the round robin algorithm.
    async fn round_robin(pool: &ServerPool) -> &Server {
        let mut r_curr = pool.current.write().await;
        loop {
            let (server, server_info) = &pool.servers[*r_curr];
            let server_info = server_info.read().await;
            if !server_info.alive {
                println!("(Stabilize) Server is not alive: {}", server.get_quic());
                *r_curr += 1;
                if *r_curr == pool.servers.len() {
                    *r_curr = 0;
                }
            } else {
                *r_curr += 1;
                return server;
            }
        }
    }

    async fn least_connections(pool: &ServerPool) -> &Server {
        let len = &pool.servers.len();
        let mut server_place = 0;
        let mut least_connections = 255;
        for i in 0..*len {
            let (server, server_info) = &pool.servers[i];
            let server_info = server_info.read().await;
            if !server_info.alive {
                println!("(Stabilize) Server is not alive: {}", server.get_quic());
            } else {
                if server_info.connections < least_connections {
                    server_place = i;
                    least_connections = server_info.connections;
                }
            }
        }
        let (server, server_info) = &pool.servers[server_place];
        let mut server_info = server_info.write().await;
        server_info.connections += 1;
        server
    }

    /// Function to help with least connections cleanup and will find entry for server info and will
    /// decrement the number of active connections.
    pub async fn decrement_connections(pool: &ServerPool, server_addr: SocketAddr){
        // Go into server pool and find info for server
        // get the write lock and decrement connections
        for (server, server_info) in &pool.servers {
            if server.get_quic() == server_addr {
                let mut server_info = server_info.write().await;
                server_info.connections -= 1;
            }
        }

    }
}

/// Function to encapsulate the state of a server
#[derive(Deserialize, Debug, Copy, Clone)]
pub struct ServerInfo {
    pub alive: bool,
    pub connections: u8
}

impl ServerInfo {
    /// Will create a new ServerInfo object.
    pub fn new() -> ServerInfo {
        ServerInfo { alive: true, connections: 0 }
    }
}

/// Server object. Contains server address but in the
/// future will contain more extensive information about
/// the servers.
#[derive(Deserialize, Debug, Copy, Clone)]
pub struct Server {
    quic: SocketAddr,
    heartbeat: SocketAddr,
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
    pub servers: Vec<(Server, RwLock<ServerInfo>)>,
    current: RwLock<usize>,
    pub algo: Algo,
}

/// ServerPool functions
impl ServerPool {
    /// This function will go through a config file which contains the
    /// servers it needs to connect to and will build a ServerPool instance
    /// using these server addresses
    pub fn create_from_file(config: &str, algo: Algo) -> ServerPool {
        // Open up file from config path
        // Go through the config and create a HashMap which contains Server structs
        // based on the addresses in the config file
        let mut config_toml = String::from("");
        let mut file = match File::open(&config) {
            Ok(file) => file,
            Err(_) => {
                panic!("(Stabilize) Could not find config file, using default!");
            }
        };
        file.read_to_string(&mut config_toml)
            .unwrap_or_else(|err| panic!("(Stabilize) Error while reading config: [{}]", err));

        let config: Config = toml::from_str(&config_toml).unwrap();

        let servers = config.servers;

        println!("{:?}", servers);

        let list: Vec<_> = servers
            .iter()
            .map(|s| (s.clone(), RwLock::new(ServerInfo::new())))
            .collect();
        ServerPool {
            servers: list,
            current: RwLock::new(0),
            algo,
        }
    }

    /// Create a new, blank ServerPool object
    pub fn new() -> ServerPool {
        ServerPool {
            servers: Vec::new(),
            current: RwLock::new(0),
            algo: Algo::RoundRobin,
        }
    }

    /// Add a Server to the ServerPool
    pub fn add(&mut self, server: Server) {
        self.servers.push((server, RwLock::new(ServerInfo::new())));
    }

    /// Function to get the next Server from the ServerPool.
    /// This is the function which implements the LB algorithms.
    pub async fn get_next(pool: &ServerPool) -> &Server {
        println!("(Stabilize) Getting a server");
        match pool.algo {
            Algo::RoundRobin => Algo::round_robin(pool).await,
            Algo::LeastConnections => Algo::least_connections(pool).await
        }
    }

    /// Function to check if a server is alive at the specified addr port. It will send a short
    /// message to the port and will wait for a response. If there is no response, it will assume
    /// the server is dead and will move on.
    pub async fn heartbeat(addr: SocketAddr, home: SocketAddr) -> bool {
        let mut sock = UdpSocket::bind(home).await.expect(&format!(
            "(Stabilize Health) Couldn't bind socket to address {}",
            addr
        ));
        match sock.connect(addr).await {
            Ok(_) => println!("(Stabilize Health) Connected to address: {}", addr),
            Err(_) => println!("(Stabilize Health) Did not connect to address: {}", addr),
        };
        sock.send("a".as_bytes()).await.unwrap();
        let mut buf = [0; 1];
        match sock.recv(&mut buf).await {
            Ok(_) => {
                println!(
                    "(Stabilize Health) Received: {:?}, Server Alive {}",
                    &buf, &addr
                );
                true
            }
            Err(_) => {
                println!("(Stabilize Health) Server dead: {}", &addr);
                false
            }
        }
    }

    /// Function to update a specified server info struct with information about that server
    pub async fn update_server_info(server: &Server, home: SocketAddr, info: &mut ServerInfo) {
        println!(
            "(Stabilize Health) Changing the status of {}",
            server.get_quic()
        );
        info.alive = ServerPool::heartbeat(server.get_hb(), home).await;
        println!("(Stabilize Health) Alive: {}", info.alive);
    }

    /// This function will go through a serverpool and check the health of each server
    pub async fn check_health(serverpool: Arc<ServerPool>, home: SocketAddr) {
        println!("(Stabilize Health) This function will start to check the health of servers in the server pool");
        // Loop through all servers in serverpool
        for (server, servinfo) in &serverpool.servers {
            // Run check on each server
            println!(
                "(Stabilize Health) Quic: {}, HB: {}",
                &server.quic, &server.heartbeat
            );
            let status;
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
    pub async fn check_health_runner(serverpool: Arc<ServerPool>, home: SocketAddr, delay: u64) {
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
        let cert_path = PathBuf::from("cert.der");
        let cert = match std::fs::read(&cert_path) {
            Ok(x) => x,
            Err(e) => {
                panic!("failed to read certificate: {}", e);
            }
        };
        // let cert = quinn::Certificate::from_der(&cert)?;
        let mut client_config = ServerConnect::configure_client(&[&cert]).unwrap();
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

    fn configure_client(
        server_certs: &[&[u8]],
    ) -> Result<quinn::ClientConfigBuilder, Box<dyn Error>> {
        let mut cfg_builder = quinn::ClientConfigBuilder::default();
        for cert in server_certs {
            cfg_builder.add_certificate_authority(quinn::Certificate::from_der(&cert)?)?;
        }
        Ok(cfg_builder)
    }
}

#[cfg(test)]
mod tests;
