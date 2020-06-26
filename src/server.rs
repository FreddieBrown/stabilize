use std::net::SocketAddr;
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};

#[derive(Debug)]
#[derive(Hash)]
pub struct Server {
    addr: SocketAddr,
}

impl Server {
    pub fn new(addr: SocketAddr) -> Server{
        Server {
            addr
        }
    }

    pub fn create_from_file() -> HashMap<Server, Option<std::net::SocketAddr>>{
        // Open up file from config path
        // Go through the config and create a HashMap which contains Server structs
        // based on the addresses in the config file
        let mut map = HashMap::new();
        let file = File::open(".config").unwrap();
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let addr = line.unwrap();
            println!("{}", &addr);
            let addr = addr.parse::<SocketAddr>().unwrap();
            map.insert(Server::new(addr), None);
        }
        map
    }
}

impl PartialEq for Server {
    fn eq(&self, other: &Self) -> bool {
        self.addr == other.addr
    }
}

impl Eq for Server {}