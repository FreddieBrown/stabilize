use std::net::SocketAddr;
use std::io;
use tokio::net::UdpSocket;

pub struct Server {
    socket: UdpSocket,
    buf: Vec<u8>,
    to_send: Option<(usize, SocketAddr)>,
}

impl Server {

    pub fn new(socket:UdpSocket, buf: Vec<u8>, to_send: Option<(usize, SocketAddr)>) -> Server {
        Server{
            socket,
            buf,
            to_send
        }
    }

    pub async fn run(self) -> Result<(), io::Error> {
        let Server {
            mut socket,
            mut buf,
            mut to_send,
        } = self;

        loop {
            // First we check to see if there's a message we need to echo back.
            // If so then we try to send it back to the original source, waiting
            // until it's writable and we're able to do so.
            if let Some((size, peer)) = to_send {
                let amt = socket.send_to(&buf[..size], &peer).await?;

                println!("Echoed {}/{} bytes to {}", amt, size, peer);
            }

            // If we're here then `to_send` is `None`, so we take a look for the
            // next message we're going to echo back.
            to_send = Some(socket.recv_from(&mut buf).await?);
        }
    }
}