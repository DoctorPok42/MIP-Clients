//! Example usage of the MIP Client

use mip_client::{MIPClient, MIPClientOptions, FrameFlags};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let client_id = "";

    let options = MIPClientOptions::default()
        .client_id(client_id.to_string())
        .host("127.0.0.1")
        .port(9000)
        .auto_reconnect(true)
        .reconnect_delay_ms(3000)
        .max_reconnect_attempts(5)
        .ping_interval_ms(30000);

    let mut client = MIPClient::new(options);

    // Register event callbacks
    client.on_connect(|| {
        println!("✓ Connected to server!");
    });

    client.on_disconnect(|| {
        println!("✗ Disconnected from server");
    });

    client.on_reconnecting(|attempt| {
        println!("↻ Reconnecting... (attempt {})", attempt);
    });

    client.on_message(|msg| {
        println!("📨 Message on '{}': {}", msg.topic, msg.message);
    });

    client.on_event(|msg| {
        println!("📢 Event on '{}': {}", msg.topic, msg.message);
    });

    client.on_ack(|msg_id| {
        println!("✓ ACK received for message {}", msg_id);
    });

    client.on_pong(|| {
        println!("🏓 Pong received");
    });

    client.on_error(|err| {
        eprintln!("❌ Error: {}", err);
    });

    // Connect to the server
    println!("Connecting to MIP server...");
    client.connect().await?;

    // Subscribe to a topic
    let sub_id = client.subscribe("test/topic", true)?;
    println!("Subscribed with message ID: {}", sub_id);

    // Publish a message
    let pub_id = client.publish("test/topic", "Hello from Rust!", FrameFlags::NONE)?;
    println!("Published with message ID: {}", pub_id);

    // Send a ping
    let ping_id = client.ping()?;
    println!("Ping sent with message ID: {}", ping_id);

    // Wait for Ctrl+C
    println!("\nPress Ctrl+C to disconnect...\n");
    tokio::signal::ctrl_c().await?;

    // Cleanup
    client.disconnect().await?;

    Ok(())
}

