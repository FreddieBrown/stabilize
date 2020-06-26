use bytes::Bytes;
use futures::{future, Sink, SinkExt, Stream, StreamExt};
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use tokio::net::udp::{RecvHalf, SendHalf};
use tokio::net::UdpSocket;

pub struct Client {
    addr: SocketAddr
}

impl Client {
    pub async fn new(addr: SocketAddr) -> Result<Client, io::Error>{
        Ok(Client{
            addr
        })
        

    }

    pub async fn run(
        &self,
        stdin: impl Stream<Item = Result<Bytes, io::Error>> + Unpin,
        stdout: impl Sink<Bytes, Error = io::Error> + Unpin,
    ) -> Result<(), Box<dyn Error>> {

        let bind_addr = if self.addr.ip().is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = UdpSocket::bind(&bind_addr).await?;
        socket.connect(self.addr).await?;
        let (mut r, mut w) = socket.split();

        future::try_join(Client::send(stdin, &mut w), Client::recv(stdout, &mut r)).await?;

        Ok(())

    }

    async fn send (
        mut stdin: impl Stream<Item = Result<Bytes, io::Error>> + Unpin,
        writer: &mut SendHalf,
    ) -> Result<(), io::Error> {
        while let Some(item) = stdin.next().await {
            let buf = item?;
            writer.send(&buf[..]).await?;
        }
        Ok(())
    }

    async fn recv(
        mut stdout: impl Sink<Bytes, Error = io::Error> + Unpin,
        reader: &mut RecvHalf,
    ) -> Result<(), io::Error> {
        loop {
            let mut buf = vec![0; 1024];
            let n = reader.recv(&mut buf[..]).await?;

            if n > 0 {
                stdout.send(Bytes::from(buf)).await?;
            }
        }
    }
}