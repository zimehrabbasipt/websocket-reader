use tungstenite::connect;
use tungstenite::Message;
use url::Url;

fn main() {
    let url = "ws://127.0.0.1:9001";

    println!("Connecting to {}...", url);

    let (mut socket, response) = connect(Url::parse(url).expect("Invalid URL"))
        .expect("Failed to connect");

    println!("Connected! HTTP status: {}", response.status());

    // Send a few test messages
    let messages = vec![
        "Hello from Rust!",
        "WebSockets are cool",
        "This is message 3",
    ];

    for msg in &messages {
        socket
            .send(Message::Text((*msg).into()))
            .expect("Failed to send");
        println!("[SENT] {}", msg);

        // Read the echo back
        match socket.read() {
            Ok(Message::Text(text)) => println!("[RECV] {}\n", text),
            Ok(other) => println!("[RECV] {:?}\n", other),
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
        }
    }

    // Close the connection
    socket.close(None).ok();
    // Drain remaining messages until close confirmation
    loop {
        match socket.read() {
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    println!("Connection closed.");
}
