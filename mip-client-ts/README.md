# MIP-Client-ts

[![npm version](https://img.shields.io/npm/v/@mip-client/ts.svg)](https://www.npmjs.com/package/@mip-client/ts)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

TypeScript client for the MIP (MSIP) protocol - handles connections, events, errors, and auto-reconnection.

**Server implementation:** [MIP Server](https://github.com/DoctorPok42/MIP)

**Part of the MIP Client family:**

- TypeScript: [@mip-client/ts](https://www.npmjs.com/package/@mip-client/ts) (this package)
- Python: [mip-client-python](https://pypi.org/project/mip-client-python/)
- Rust: [mip-client](https://crates.io/crates/mip-client)

## Installation

```bash
npm install mip-client-ts
```

## Usage

### Basic Connection

```typescript
import { MIPClient } from "mip-client-ts";

const client = new MIPClient({
  host: "127.0.0.1",
  port: 9000,
  clientId: "my_client_123",
});

// Events
client.on("connect", () => {
  console.log("Connected to server");
});

client.on("disconnect", () => {
  console.log("Disconnected");
});

client.on("error", (err) => {
  console.error("Error:", err.message);
});

client.on("message", (msg) => {
  console.log(`[${msg.topic}] ${msg.message}`);
});

// Connect
await client.connect();
```

### Subscribe / Publish

```typescript
// Subscribe to a topic
client.subscribe("my-topic");

// Listen for messages
client.on("message", (msg) => {
  console.log(`Topic: ${msg.topic}`);
  console.log(`Message: ${msg.message}`);
});

// Publish a message
client.publish("my-topic", "Hello World!");
```

### Advanced Options

```typescript
import { MIPClient, Flags } from "mip-client-ts";

const client = new MIPClient({
  host: "127.0.0.1",
  port: 9000,
  clientId: "my_client_123",  // Optional client identifier
  autoReconnect: true,        // Auto-reconnect (default: true)
  reconnectDelay: 3000,       // Delay between reconnections (default: 3000ms)
  maxReconnectAttempts: 10,   // Max attempts (0 = infinite)
  pingInterval: 5000,         // Automatic ping (0 = disabled)
});

// Publish with flags
client.publish("urgent-topic", "Important!", Flags.URGENT | Flags.ACK_REQUIRED);

// Listen for ACKs
client.on("ack", (msgId) => {
  console.log("ACK received for:", msgId.toString());
});

// Manual ping
client.ping();
client.on("pong", () => {
  console.log("Pong received!");
});
```

### Auto-Reconnection

```typescript
client.on("reconnecting", (attempt) => {
  console.log(`Reconnection attempt #${attempt}...`);
});

client.on("connect", () => {
  // Re-subscribe after reconnection
  client.subscribe("my-topic");
});
```

### Factory Function

```typescript
import { createClient } from "mip-client-ts";

const client = createClient("127.0.0.1", 9000, {
  autoReconnect: true,
  pingInterval: 10000,
});
```

## API

### `MIPClient`

#### Constructor Options

| Option                 | Type      | Default | Description                            |
| ---------------------- | --------- | ------- | -------------------------------------- |
| `clientId`             | `string`  | `""`    | Client identifier (sent in HELLO frame)|
| `host`                 | `string`  | -       | Server address                         |
| `port`                 | `number`  | -       | Server port                            |
| `autoReconnect`        | `boolean` | `true`  | Auto-reconnect on disconnect           |
| `reconnectDelay`       | `number`  | `3000`  | Delay between reconnections (ms)       |
| `maxReconnectAttempts` | `number`  | `10`    |Max reconnection attempts (0 = infinite)|
| `pingInterval`         | `number`  | `0`     | Ping interval (0 = disabled)           |

#### Methods

- `connect(): Promise<void>` - Connect to server
- `disconnect(): void` - Disconnect from server
- `subscribe(topic: string, requireAck?: boolean): bigint` - Subscribe to a topic
- `unsubscribe(topic: string, requireAck?: boolean): bigint` - Unsubscribe from a topic
- `publish(topic: string, message: string | Buffer, flags?: number): bigint` - Publish a message
- `ping(): bigint` - Send a ping

#### Events

- `connect` - Connection established
- `disconnect` - Disconnected
- `reconnecting(attempt)` - Reconnection attempt
- `message(msg)` - Message received (PUBLISH or EVENT)
- `event(msg)` - Event received (EVENT only)
- `ack(msgId)` - ACK received
- `pong` - Pong received
- `error(error)` - Error occurred
- `frame(header, payload)` - Raw frame (advanced usage)

### Constants

```typescript
import { FrameType, Flags } from "mip-client-ts";

// Frame types
FrameType.HELLO       // 0x0001
FrameType.SUBSCRIBE   // 0x0002
FrameType.UNSUBSCRIBE // 0x0003
FrameType.PUBLISH     // 0x0004
FrameType.EVENT       // 0x0005
FrameType.ACK         // 0x0006
FrameType.ERROR       // 0x0007
FrameType.PING        // 0x0008
FrameType.PONG        // 0x0009
FrameType.CLOSE       // 0x000A

// Flags
Flags.NONE         // 0b00000000
Flags.ACK_REQUIRED // 0b00000001
Flags.COMPRESSED   // 0b00000010
Flags.URGENT       // 0b00000100
```

## License

MIT License
