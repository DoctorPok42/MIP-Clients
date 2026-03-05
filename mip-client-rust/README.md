# MIP-Client-rust

[![Crates.io](https://img.shields.io/crates/v/mip-client.svg)](https://crates.io/crates/mip-client)
[![Documentation](https://docs.rs/mip-client/badge.svg)](https://docs.rs/mip-client)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Rust client for the MIP (MSIP) protocol with async support.

**Server implementation:** [MIP Server](https://github.com/DoctorPok42/MIP)

**Part of the MIP Client family:**

- TypeScript: [@mip-client/ts](https://www.npmjs.com/package/@mip-client/ts)
- Python: [mip-client-python](https://pypi.org/project/mip-client-python/)
- Rust: [mip-client](https://crates.io/crates/mip-client) (this package)

## Features

- **Async/Await** - Built on Tokio for high-performance async I/O
- **Auto-reconnection** - Automatic reconnection with configurable retry logic
- **Event-driven** - Callback-based API for handling messages and events
- **Type-safe** - Full Rust type safety with proper error handling
- **Ping/Pong** - Configurable heartbeat interval

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mip-client = "1.0"
tokio = { version = "1", features = ["full"] }
```

Or via cargo:

```bash
cargo add mip-client
```

## Quick Start

```rust
use mip_client::{MIPClient, MIPClientOptions, FrameFlags};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = MIPClientOptions::default()
        .host("127.0.0.1")
        .port(9000)
        .auto_reconnect(true);

    let mut client = MIPClient::new(options);

    client.on_connect(|| {
        println!("Connected!");
    });

    client.on_message(|msg| {
        println!("Received: {} - {}", msg.topic, msg.message);
    });

    client.on_error(|err| {
        eprintln!("Error: {}", err);
    });

    client.connect().await?;
    client.subscribe("my/topic", true)?;
    client.publish("my/topic", "Hello!", FrameFlags::NONE)?;

    tokio::signal::ctrl_c().await?;
    client.disconnect().await?;

    Ok(())
}
```

## API Reference

### MIPClientOptions

| Option | Type | Default | Description |
| -------- | ------ | --------- | ------------- |
| `host` | String | "127.0.0.1" | Server host address |
| `port` | u16 | 9000 | Server port number |
| `auto_reconnect` | bool | true | Auto-reconnect on disconnect |
| `reconnect_delay_ms` | u64 | 3000 | Reconnect delay in ms |
| `max_reconnect_attempts` | u32 | 10 | Max reconnect attempts (0 = infinite) |
| `ping_interval_ms` | u64 | 0 | Ping interval in ms (0 = disabled) |

### Methods

| Method | Description |
| -------- | ------------- |
| `connect()` | Connect to the MIP server |
| `disconnect()` | Disconnect from the server |
| `subscribe(topic, require_ack)` | Subscribe to a topic |
| `unsubscribe(topic, require_ack)` | Unsubscribe from a topic |
| `publish(topic, message, flags)` | Publish a message to a topic |
| `ping()` | Send a ping to the server |
| `is_connected()` | Check if the client is connected |

### Events

| Event | Callback Type | Description |
| ------- | --------------- | ------------- |
| `on_connect` | `Fn()` | Called when connected |
| `on_disconnect` | `Fn()` | Called when disconnected |
| `on_reconnecting` | `Fn(u32)` | Called on reconnection attempt |
| `on_message` | `Fn(MIPMessage)` | Called on any message |
| `on_event` | `Fn(MIPMessage)` | Called on EVENT frames |
| `on_ack` | `Fn(u64)` | Called when ACK received |
| `on_pong` | `Fn()` | Called when PONG received |
| `on_error` | `Fn(MIPError)` | Called on error |
| `on_frame` | `Fn(Header, Vec<u8>)` | Called on any raw frame |

### Frame Flags

```rust
use mip_client::FrameFlags;

FrameFlags::NONE         // No flags
FrameFlags::ACK_REQUIRED // Request acknowledgment
FrameFlags::COMPRESSED   // Payload is compressed
FrameFlags::URGENT       // Urgent message
```

## Protocol

The MIP protocol uses a binary frame format with a 24-byte header:

| Field | Size | Description |
| ------- | ------ | ------------- |
| Magic | 4 bytes | "MSIP" |
| Version | 1 byte | Protocol version (1) |
| Flags | 1 byte | Frame flags |
| Frame Type | 2 bytes | Type of frame |
| Message Kind | 2 bytes | Kind of message |
| Reserved | 2 bytes | Reserved for future use |
| Payload Length | 4 bytes | Length of payload |
| Message ID | 8 bytes | Unique message identifier |

## License

MIT
