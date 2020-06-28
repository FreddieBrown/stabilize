use std::{
    net::SocketAddr,
    fs::File,
    io::{prelude::*, BufReader}
};

use tokio::sync::RwLock;

pub struct ServerInfo {
    alive: bool
}

impl ServerInfo {
    pub fn new () -> ServerInfo{
        ServerInfo {
            alive: false
        }
    }
}

pub struct Server {
    addr: SocketAddr
}

impl Server {
    pub fn new(addr: SocketAddr) -> Server {
        Server{
            addr
        }
    }
}

pub struct ServerPool {
    servers: Vec<(Server, RwLock<ServerInfo>)>,
    current: RwLock<usize>
}

impl ServerPool {
    pub fn create_from_file() -> ServerPool {
        // Open up file from config path
        // Go through the config and create a HashMap which contains Server structs
        // based on the addresses in the config file
        let mut list = Vec::new();
        let file = File::open(".config").unwrap();
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let addr = line.unwrap();
            println!("{}", &addr);
            let addr = addr.parse::<SocketAddr>().unwrap();
            list.push((Server::new(addr), RwLock::new(ServerInfo::new())));
        }
        ServerPool {
            servers: list,
            current: RwLock::new(0)
        }
    }

    pub fn new() -> ServerPool {
        ServerPool{
            servers: Vec::new(),
            current: RwLock::new(0)
        }
    }


    pub async fn get_next(&self) -> &Server{
        let mut r_curr = self.current.write().await;

        loop {
            let (server, server_info) = &self.servers[*r_curr];
            let server_info = server_info.read().await;
            if !server_info.alive {
                *r_curr += 1;

                if *r_curr == self.servers.len() {
                    *r_curr = 0;
                }
            }
            else {
                *r_curr += 1;
                return server;
            }
        }
    }

    // Write health checking functions

}