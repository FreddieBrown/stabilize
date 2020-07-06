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
    let home: SocketAddr = "127.0.0.1:5001".parse().unwrap();
    let verdict = ServerPool::check_conn(to_connect, home, Duration::new(1,0)).await;
    assert!(verdict);
}

#[tokio::test]
async fn test_check_update_server_info() {
    tokio::spawn( async move {
        let home: SocketAddr = "127.0.0.1:5003".parse().unwrap();
        match UdpSocket::bind(home).await {
            Ok(_) => println!("Connected!"),
            Err(_) => println!("No Connection!")
        };
    });
    let to_connect: Server = Server::new("127.0.0.1:5003".parse().unwrap());
    let home: SocketAddr = "127.0.0.1:5004".parse().unwrap();
    let mut info = ServerInfo::new();
    info.alive = false;
    ServerPool::update_server_info(&to_connect, home, &mut info, Duration::new(1,0)).await;
    assert_eq!(true, info.alive);
}

#[tokio::test]
async fn test_check_health() {
    assert_eq!(1, 1)
}

