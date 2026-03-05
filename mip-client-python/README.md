# MIP-Client-python

[![PyPI version](https://img.shields.io/pypi/v/mip-client-python.svg)](https://pypi.org/project/mip-client-python/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Python async client for the MIP (MSIP) protocol - handles connections, events, errors, and auto-reconnection.

**Server implementation:** [MIP Server](https://github.com/DoctorPok42/MIP)

**Part of the MIP Client family:**

- TypeScript: [@mip-client/ts](https://www.npmjs.com/package/@mip-client/ts)
- Python: [mip-client-python](https://pypi.org/project/mip-client-python/) (this package)
- Rust: [mip-client](https://crates.io/crates/mip-client)

## Installation

```bash
pip install mip-client-python
```

## Usage

### Basic Connection

```python
import asyncio
from mip_client import MIPClient

async def main():
    client = MIPClient(host="127.0.0.1", port=9000)

    # Register event callbacks
    client.on_connect(lambda: print("Connected to server"))
    client.on_disconnect(lambda: print("Disconnected"))
    client.on_error(lambda err: print(f"Error: {err.message}"))
    client.on_message(lambda msg: print(f"[{msg.topic}] {msg.message}"))

    # Connect
    await client.connect()

    # Keep running
    await asyncio.sleep(60)
    await client.disconnect()

asyncio.run(main())
```

### Subscribe / Publish

```python
# Subscribe to a topic
client.subscribe("my-topic")

# Listen for messages
def handle_message(msg):
    print(f"Topic: {msg.topic}")
    print(f"Message: {msg.message}")

client.on_message(handle_message)

# Publish a message
client.publish("my-topic", "Hello World!")
```

### Advanced Options

```python
from mip_client import MIPClient, Flags

client = MIPClient(
    host="127.0.0.1",
    port=9000,
    auto_reconnect=True,        # Auto-reconnect (default: True)
    reconnect_delay=3.0,        # Delay between reconnections in seconds (default: 3.0)
    max_reconnect_attempts=10,  # Max attempts (0 = infinite)
    ping_interval=5.0,          # Ping interval in seconds (0 = disabled)
)

# Publish with flags
client.publish("urgent-topic", "Important!", Flags.URGENT | Flags.ACK_REQUIRED)

# Listen for ACKs
client.on_ack(lambda msg_id: print(f"ACK received for: {msg_id}"))

# Manual ping
client.ping()
client.on_pong(lambda: print("Pong received!"))
```

### Auto-Reconnection

```python
client.on_reconnecting(lambda attempt: print(f"Reconnection attempt #{attempt}..."))

def on_connect():
    # Re-subscribe after reconnection
    client.subscribe("my-topic")

client.on_connect(on_connect)
```

### Using create_client helper

```python
from mip_client import create_client

client = create_client("127.0.0.1", 9000, auto_reconnect=True)
```

## API Reference

### MIPClient

- `connect()` - Connect to the server (async)
- `disconnect()` - Disconnect from the server (async)
- `subscribe(topic, require_ack=True)` - Subscribe to a topic
- `unsubscribe(topic, require_ack=True)` - Unsubscribe from a topic
- `publish(topic, message, flags=Flags.NONE)` - Publish a message
- `ping()` - Send a ping to the server

### Events

- `on_connect(callback)` - Called when connected
- `on_disconnect(callback)` - Called when disconnected
- `on_reconnecting(callback)` - Called when reconnecting (receives attempt number)
- `on_message(callback)` - Called when a message is received
- `on_event(callback)` - Called when an event is received
- `on_ack(callback)` - Called when an ACK is received
- `on_pong(callback)` - Called when a pong is received
- `on_error(callback)` - Called when an error occurs

### Flags

- `Flags.NONE` - No flags
- `Flags.ACK_REQUIRED` - Request acknowledgment
- `Flags.COMPRESSED` - Compressed payload
- `Flags.URGENT` - Urgent message
