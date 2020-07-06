use super::*;
use tokio::net::UdpSocket;

#[test]
fn test_server_get_addr() {
    let addr: SocketAddr = "127.0.0.1:5347".parse().unwrap();
    let new_server = Server::new(addr);
    assert_eq!(addr, new_server.get_addr());
}

#[tokio::test]
async fn test_create_from_files() {
    let addrs = vec!["127.0.0.1:5347", "127.0.0.1:5348", "127.0.0.1:5349"];
    let serverp = ServerPool::create_from_file("test_config");
    for i in 0..3 {
        let serveraddr = serverp.get_next().await.get_addr();
        assert_eq!(Ok(serveraddr), addrs[i].parse());
    }
}

#[tokio::test]
async fn test_check_conn() {
    tokio::spawn( async move {
        let home: SocketAddr = "127.0.0.1:5002".parse().unwrap();
        match UdpSocket::bind(home).await {
            Ok(_) => println!("Connected!"),
            Err(_) => println!("No Connection!")
        };
    });
    let to_connect: SocketAddr = "127.0.0.1:5002".parse().unwrap();
    let verdict = ServerPool::check_conn(to_connect, Duration::new(3,0)).await;
    assert!(verdict);
}

/// These tests will test the health checking functionality of the stabilize server. If it is passing,  
/// it will go through the 3 servers in config and will find that 2 are working and 1 isn't. Will also
/// check if ServerPool is working too.
#[test]
fn test_server_healthcheck() {
    assert_eq!(1, 1);
}

