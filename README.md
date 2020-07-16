# Stabilize

[![Build Status](https://travis-ci.com/FreddieBrown/stabilize.svg?branch=master)](https://travis-ci.com/FreddieBrown/stabilize)

## QUIC load balancer written in Rust

To run, use the command `cargo run -- --listen 5000`.

To use the test client and server, use `cargo run --bin <choice>` where `<choice>` is replaced by either `client` or `server`.

To have clients associated with `Stabilize`, put their details in a `.config.toml` file. The structure of this is:

```toml
servers = [
    {quic = "127.0.0.1:5347", heartbeat = "127.0.0.1:6347"},
    {quic = "127.0.0.1:5348", heartbeat = "127.0.0.1:6348"},
    {quic = "127.0.0.1:5349", heartbeat = "127.0.0.1:6349"}
]
```
The `quic` address is that which the `Stabilize` balancer will connect to and communicate over mainly. The `heartbeat` port is the one used for server health checking to determine if a fault has occured in a server. This is used regularly by `Stabilize` to ensure the program can service client requests.


For help: 

```
stabilize 0.1.0

USAGE:
    stabilize [FLAGS] [OPTIONS]

FLAGS:
    -h, --help               Prints help information
        --keylog             file to log TLS keys to for debugging
        --stateless-retry    Enable stateless retries
    -V, --version            Prints version information

OPTIONS:
    -c, --cert <cert>        TLS certificate in PEM format
    -k, --key <key>          TLS private key in PEM format
        --listen <listen>    Address to listen on [default: 4433]
```

The program uses `cert.der` and `key.der` by default as its certificate and key. Any custom certificate and key should be named as such for the program to work. If there is no certificate and/or key, Stabilize will create and sign a pair itself.

## Plan

Architecture diagram [here](https://drive.google.com/file/d/1LoCD13TSaLTHX2yjudHgg3aJ7d42NlO7/view?usp=sharing)

### Client

This is a UDP client that wants to communicate with the service using the load balancer. This will send a UDP packet to the web address (which will go to the load balancer) and will expect a response. 

### Server

This is a regular UDP service. There are multiple servers connected to the load balancer, each of these is running the exact same service. This means that the load balancer can see each of them as providing the same service, so any of them can service a UDP packet in the same way. 

### Stabilize

This is the actual load balancer. It acts as a middleman for a connection and helps servers not to get overwhelmed. It also enables a web service to increase headroom easily by allowing clients to connect to a number of different servers. This will, on average, lighten the load of any one server. 

This contains a listener, which will listen out for any UDP packets sent by the Client. The Stabilize instance will then decide which Server to pass on the packet using the Round Robin algorithm. This works by passes packet 1 to Server 1, packet 2 to Server 2, ..., and packet N to Server N. It will then go  back round to the start, passing packet N+1 to Server 1. This helps to spread the packet load across the servers evenly. 

## Contributing

To contribute, send a PR and write tests. An aim of the project is to have extensive testing so that it is ensured that it will all work. 

Additionally, write comments to described what is being done in a function/complex code block. This helps people understand what is going on if they need to change/replicate the behaviour. 
