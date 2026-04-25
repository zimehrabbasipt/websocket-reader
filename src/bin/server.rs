use std::net::TcpListener;
use tungstenite::accept;
use tungstenite::Message;

fn main() {
    let addr = "127.0.0.1:9001";
    let listener = TcpListener::bind(addr).expect("Failed to bind");

    println!("Echo server listening on ws://{}", addr);
    println!("Waiting for a connection...\n");

    for stream in listener.incoming() {
        let stream = stream.expect("Failed to accept");
        let peer = stream.peer_addr().unwrap();
        println!("[+] New connection from {}", peer);

        let mut ws = accept(stream).expect("WebSocket handshake failed");

        loop {
            match ws.read() {
                Ok(msg) => match msg {
                    Message::Text(text) => {
                        println!("[ECHO] {}", text);
                        ws.send(Message::Text(text)).unwrap();
                    }
                    Message::Binary(bin) => {
                        println!("[ECHO] {} bytes", bin.len());
                        ws.send(Message::Binary(bin)).unwrap();
                    }
                    Message::Ping(data) => {
                        ws.send(Message::Pong(data)).unwrap();
                    }
                    Message::Close(_) => {
                        println!("[-] {} disconnected", peer);
                        break;
                    }
                    _ => {}
                },
                Err(e) => {
                    println!("[-] {} error: {}", peer, e);
                    break;
                }
            }
        }
    }
}
