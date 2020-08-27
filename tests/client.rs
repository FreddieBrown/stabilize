use anyhow::{anyhow, Context, Result};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::{error::Error, future::Future, path::PathBuf};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use structopt::{self, StructOpt};

pub const CUSTOM_PROTO: &[&[u8]] = &[b"cstm-01"];

#[derive(Clone)]
pub struct QuicClient {
    endpoint: quinn::Endpoint,
    conn: quinn::Connection,
}

impl QuicClient {
    /// Creates a new QuicClient that does not verify certificates. Used mainly for testing.
    pub async fn new(port: u16, connect: &str) -> Result<QuicClient> {
        QuicClient::create(port, connect).await
    }

    #[doc(hidden)]
    async fn create(port: u16, connect: &str) -> Result<QuicClient> {
        let addr: SocketAddr = connect.parse()?;

        let cert_path = PathBuf::from("cert.der");
        let cert = match std::fs::read(&cert_path) {
            Ok(x) => x,
            Err(e) => {
                panic!("failed to read certificate: {}", e);
            }
        };
        // let cert = quinn::Certificate::from_der(&cert)?;
        let mut client_config = QuicClient::configure_client(&[&cert]).unwrap();

        client_config.protocols(CUSTOM_PROTO);

        let socket_addr: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
        let (endpoint, _) = quinn::Endpoint::builder()
            .bind(&socket_addr)
            .context("(Client) Could not bind client endpoint")?;

        let conn = endpoint
            .connect_with(client_config.build(), &addr, "localhost")?
            .await
            .context(format!("(Client) Could not connect to {}", &addr))?;

        let quinn::NewConnection {
            connection: conn, ..
        } = { conn };

        Ok(QuicClient { endpoint, conn })
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

    #[doc(hidden)]
    async fn open_new_connection(&mut self) -> Result<(quinn::SendStream, quinn::RecvStream)> {
        Ok(self.conn.open_bi().await?)
    }

    /// Make the request to the remote peer and receive a response.
    pub async fn make_request(&mut self, msg: &str) -> anyhow::Result<String> {
        let (mut send, mut recv) = self.open_new_connection().await?;

        async fn do_request(
            msg: &str,
            send: &mut quinn::SendStream,
            recv: &mut quinn::RecvStream,
        ) -> Result<String> {
            // send the request...
            println!("(Client) Sending request...");
            send.write_all(msg.as_bytes()).await?;
            send.finish().await?;
            println!("(Client) Request sent!");

            // ...and return the response.
            println!("(Client) Reading response...");
            let mut incoming = bytes::BytesMut::new();
            let mut recv_buffer = [0 as u8; 1024]; // 1 KiB socket recv buffer
            let mut msg_size = 0;

            while let Some(s) = recv
                .read(&mut recv_buffer)
                .await
                .map_err(|e| anyhow!("Could not read message from recv stream: {}", e))?
            {
                println!("(Client) Got data back");
                msg_size += s;
                incoming.extend_from_slice(&recv_buffer[0..s]);
            }

            let frozen = incoming.freeze();
            let ret = std::str::from_utf8(frozen.as_ref())?;
            println!(
                "(Client) Received response {} bytes long from server: {}",
                msg_size, ret
            );

            Ok(String::from(ret))
        }

        Ok(do_request(msg, &mut send, &mut recv)
            .await
            .map_err(|e| anyhow!("(Client) Making request failed: {}", e))?)
    }

    pub fn close(&self) {
        self.endpoint.close(0u8.into(), b"done");
    }
}

/// generates three futures that make the same request for each client passed in.
pub fn generate_futures(
    client: &QuicClient,
) -> FuturesUnordered<impl Future<Output = anyhow::Result<String>>> {
    let requests = FuturesUnordered::new();

    for _ in 0..1 {
        let mut cloned = client.clone();
        requests.push(async move { cloned.make_request("Hello, world!").await })
    }

    requests
}


#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "stabilize-client")]
pub struct Opt {
    /// Port to connect over
    #[structopt(long = "connect", default_value = "60612")]
    connect: u16,
    /// Address to connect to
    #[structopt(long = "addr", default_value="127.0.0.1:5000")]
    addr: String 
}

#[tokio::main]
async fn main() -> Result<()> {

    let opt = Opt::from_args();
    let client = QuicClient::new(opt.connect, &opt.addr[..]).await?;

    for _ in 1..2 {
        let mut requests = generate_futures(&client);

        let mut finished_ops = 0;
        let max_finished_ops = 1;

        while finished_ops < max_finished_ops {
            match requests.next().await {
                Some(_) => finished_ops += 1,
                None => println!(
                    "(Client) Finished the stream before getting enough ops: {} vs {}",
                    finished_ops, max_finished_ops
                ),
            }
        }
    }

    client.close();

    Ok(())
}
