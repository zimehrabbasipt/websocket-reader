use tungstenite::connect;
use tungstenite::Message;
use url::Url;

fn main() {
    let url = "wss://echo.websocket.org";

    println!("Connecting to {}...", url);

    let (mut socket, response) = connect(Url::parse(url).expect("Invalid URL"))
        .expect("Failed to connect");

    println!("Connected! HTTP status: {}", response.status());

    // Send a test message
    socket
        .send(Message::Text("Hello from Rust!".into()))
        .expect("Failed to send");

    println!("Sent: Hello from Rust!");
    println!("Waiting for messages...\n");

    // Read loop — blocks until a message arrives
    loop {
        match socket.read() {
            Ok(msg) => match msg {
                Message::Text(text) => println!("[TEXT] {}", text),
                Message::Binary(bin) => println!("[BIN]  {} bytes", bin.len()),
                Message::Ping(_) => println!("[PING]"),
                Message::Pong(_) => println!("[PONG]"),
                Message::Close(frame) => {
                    println!("[CLOSE] {:?}", frame);
                    break;
                }
                _ => {}
            },
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
        }
    }

    println!("Connection closed.");
}
