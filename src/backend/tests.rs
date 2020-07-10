use super::*;
use tokio::net::UdpSocket;

#[test]
fn test_server_get_addr() {
    let quic: SocketAddr = "127.0.0.1:5347".parse().unwrap();
    let hb: SocketAddr = "127.0.0.1:5348".parse().unwrap();
    let new_server = Server::new(quic, hb);
    assert_eq!(quic, new_server.get_quic());
    assert_eq!(hb, new_server.get_hb());
}

#[tokio::test]
async fn test_create_from_files() {
    let addrs = vec!["127.0.0.1:5347", "127.0.0.1:5348", "127.0.0.1:5349"];
    let serverp = ServerPool::create_from_file("test_data/test_config1.toml", Algo::RoundRobin);
    for i in 0..3 {
        let serveraddr = ServerPool::get_next(&serverp).await.get_quic();
        assert_eq!(Ok(serveraddr), addrs[i].parse());
    }
}

#[tokio::test]
async fn test_heartbeat() {
    println!("Running HB test");
    tokio::spawn(async move {
        let home: SocketAddr = "127.0.0.1:5002".parse().unwrap();
        let mut sock = UdpSocket::bind(home).await.unwrap();
        let mut buf = [0; 1];
        let from = match sock.recv_from(&mut buf).await {
            Ok((_, p)) => p,
            Err(_) => panic!("Failure while receiving"),
        };
        println!("(Stabilize Test) Received: {:?}", &buf);
        sock.send_to("b".as_bytes(), from).await.unwrap();
    });
    let to_connect: SocketAddr = "127.0.0.1:5002".parse().unwrap();
    let home: SocketAddr = "127.0.0.1:5001".parse().unwrap();
    let verdict = ServerPool::heartbeat(to_connect, home).await;
    assert!(verdict);
}

#[tokio::test]
async fn test_fail_heartbeat() {
    let to_connect: SocketAddr = "127.0.0.1:5006".parse().unwrap();
    let home: SocketAddr = "127.0.0.1:5005".parse().unwrap();
    let verdict = ServerPool::heartbeat(to_connect, home).await;
    assert_eq!(verdict, false);
}

#[tokio::test]
async fn test_check_update_server_info() {
    tokio::spawn(async move {
        let home: SocketAddr = "127.0.0.1:5003".parse().unwrap();
        let mut sock = UdpSocket::bind(home).await.unwrap();
        let mut buf = [0; 1];
        let from = match sock.recv_from(&mut buf).await {
            Ok((_, p)) => p,
            Err(_) => panic!("(Stabilize Test) Failure while receiving"),
        };
        println!("(Stabilize Test) Received: {:?}", &buf);
        sock.send_to("b".as_bytes(), from).await.unwrap();
    });
    let to_connect: Server = Server::new("127.0.0.1:5002".parse().unwrap(), "127.0.0.1:5003".parse().unwrap());
    let home: SocketAddr = "127.0.0.1:5004".parse().unwrap();
    let mut info = ServerInfo::new();
    info.alive = false;
    ServerPool::update_server_info(&to_connect, home, &mut info).await;
    assert_eq!(true, info.alive);
}

#[tokio::test]
async fn test_check_health() {
    tokio::spawn(async move {
        let home: SocketAddr = "127.0.0.1:43595".parse().unwrap();
        let mut sock = UdpSocket::bind(home).await.unwrap();
        let mut buf = [0; 1];
        let from = match sock.recv_from(&mut buf).await {
            Ok((_, p)) => p,
            Err(_) => panic!("(Stabilize Test) Failure while receiving"),
        };
        println!("(Stabilize Test) Received: {:?}", &buf);
        sock.send_to("b".as_bytes(), from).await.unwrap();
    });

    let home: SocketAddr = "127.0.0.1:43594".parse().unwrap();
    let serverpool = Arc::new(ServerPool::create_from_file("test_data/test_config2.toml", Algo::RoundRobin));
    ServerPool::check_health(serverpool.clone(), home).await;
    let sp_check = serverpool.clone();
    let (_, serveinfo1) = &sp_check.servers[0];
    let (_, serveinfo2) = &sp_check.servers[1];
    let readinfo1 = serveinfo1.read().await;
    let readinfo2 = serveinfo2.read().await;
    assert_eq!(readinfo1.alive, true);
    assert_eq!(readinfo2.alive, false);
}

#[tokio::test]
async fn test_check_health_runner() {
    let serverpool = Arc::new(ServerPool::create_from_file("test_data/test_config3.toml", Algo::RoundRobin));
    let sp_clone = serverpool.clone();
    tokio::spawn(async move {
        let home: SocketAddr = "127.0.0.1:29000".parse().unwrap();
        ServerPool::check_health_runner(sp_clone, home, 5).await;
    });

    let home: SocketAddr = "127.0.0.1:29001".parse().unwrap();
    let mut sock = UdpSocket::bind(home).await.unwrap();
    let mut buf = [0; 1];
    let from = match sock.recv_from(&mut buf).await {
        Ok((_, p)) => p,
        Err(_) => panic!("(Stabilize Test) Failure while receiving"),
    };
    println!("(Stabilize Test) Received: {:?}", &buf);
    sock.send_to("b".as_bytes(), from).await.unwrap();
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let sp_check = serverpool.clone();
    let (_, serveinfo1) = &sp_check.servers[0];
    let (_, serveinfo2) = &sp_check.servers[1];
    let readinfo1 = serveinfo1.read().await;
    let readinfo2 = serveinfo2.read().await;
    assert_eq!(readinfo1.alive, true);
    assert_eq!(readinfo2.alive, false);
}
