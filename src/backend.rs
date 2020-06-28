use std::{
    net::SocketAddr
};
use std::collections::HashMap;

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
    servers: Vec<(Server, ServerInfo)>,
    current: u8
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
            list.push((Server::new(addr), ServerInfo::new()));
        }
        ServerPool {
            servers: list,
            current: 0
        }
    }

    pub fn new() -> ServerPool {
        ServerPool{
            servers: Vec::new(),
            current: 0
        }
    }
}