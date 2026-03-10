import * as net from "node:net";
import { EventEmitter } from "node:events";

// ============================================================================
// Constants
// ============================================================================

const MAGIC = 0x4d534950; // "MSIP"
const VERSION = 1;
const HEADER_SIZE = 24;

/** Frame types for the MIP protocol */
export const FrameType = {
  HELLO: 0x0001,
  SUBSCRIBE: 0x0002,
  UNSUBSCRIBE: 0x0003,
  PUBLISH: 0x0004,
  EVENT: 0x0005,
  ACK: 0x0006,
  ERROR: 0x0007,
  PING: 0x0008,
  PONG: 0x0009,
  CLOSE: 0x000a,
} as const;

export type FrameTypeValue = (typeof FrameType)[keyof typeof FrameType];

/** Flags for frame options */
export const Flags = {
  NONE: 0b0000_0000,
  ACK_REQUIRED: 0b0000_0001,
  COMPRESSED: 0b0000_0010,
  URGENT: 0b0000_0100,
} as const;

export type FlagsValue = (typeof Flags)[keyof typeof Flags];

const MSG_KIND_EVENT = 0x0001;

// ============================================================================
// Types & Interfaces
// ============================================================================

/** Configuration options for the MIP client */
export interface MIPClientOptions {
  /** Unique client identifier (optional) */
  clientId?: string;
  /** Server host address */
  host: string;
  /** Server port number */
  port: number;
  /** Auto-reconnect on disconnect (default: true) */
  autoReconnect?: boolean;
  /** Reconnect delay in milliseconds (default: 3000) */
  reconnectDelay?: number;
  /** Maximum reconnection attempts (default: 10, 0 = infinite) */
  maxReconnectAttempts?: number;
  /** Enable ping interval in milliseconds (default: 0 = disabled) */
  pingInterval?: number;
}

/** Parsed frame header */
export interface FrameHeader {
  magic: number;
  version: number;
  flags: number;
  frameType: FrameTypeValue;
  msgKind: number;
  payloadLength: number;
  msgId: bigint;
}

/** Received message event */
export interface MIPMessage {
  header: FrameHeader;
  topic: string;
  message: string;
}

/** Error details */
export interface MIPError {
  code?: number;
  message: string;
  raw?: Buffer;
}

/** Events emitted by the MIP client */
export interface MIPClientEvents {
  connect: () => void;
  disconnect: () => void;
  reconnecting: (attempt: number) => void;
  message: (msg: MIPMessage) => void;
  event: (msg: MIPMessage) => void;
  ack: (msgId: bigint) => void;
  pong: () => void;
  error: (error: MIPError) => void;
  frame: (header: FrameHeader, payload: Buffer) => void;
}

// ============================================================================
// MIP Client Class
// ============================================================================

export class MIPClient extends EventEmitter {
  private socket: net.Socket | null = null;
  private buffer: Buffer = Buffer.alloc(0);
  private readonly options: Required<MIPClientOptions>;
  private clientId: string;
  private connected: boolean = false;
  private reconnectAttempts: number = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private pingTimer: ReturnType<typeof setInterval> | null = null;
  private msgIdCounter: bigint = 0n;

  constructor(options: MIPClientOptions) {
    super();
    this.clientId = options.clientId ?? "";
    this.options = {
      clientId: this.clientId,
      host: options.host,
      port: options.port,
      autoReconnect: options.autoReconnect ?? true,
      reconnectDelay: options.reconnectDelay ?? 3000,
      maxReconnectAttempts: options.maxReconnectAttempts ?? 10,
      pingInterval: options.pingInterval ?? 0,
    };
  }

  // --------------------------------------------------------------------------
  // Public API
  // --------------------------------------------------------------------------

  /** Check if the client is connected */
  get isConnected(): boolean {
    return this.connected;
  }

  /** Connect to the MIP server */
  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      if (this.connected) {
        resolve();
        return;
      }

      this.socket = new net.Socket();
      this.buffer = Buffer.alloc(0);

      const onConnect = () => {
        this.connected = true;
        this.reconnectAttempts = 0;
        this.setupPingInterval();
        this.emit("connect");

        const payload = Buffer.from(this.clientId, "utf-8");
        try {
          const msgId = this.sendFrame(FrameType.HELLO, payload, Flags.NONE);
          this.clientId = msgId.toString();
        } catch (err) {
          this.emitError({ message: `Failed to send HELLO frame: ${err}` });
        }

        resolve();
      };

      const onError = (err: Error) => {
        if (!this.connected) {
          reject(err);
        }
        this.emitError({ message: err.message });
      };

      this.socket.once("connect", onConnect);
      this.socket.once("error", onError);

      this.socket.on("data", this.handleData.bind(this));
      this.socket.on("close", this.handleClose.bind(this));
      this.socket.on("error", (err) => {
        if (this.connected) {
          this.emitError({ message: err.message });
        }
      });

      this.socket.connect(this.options.port, this.options.host);
    });
  }

  /** Disconnect from the server */
  disconnect(): void {
    this.options.autoReconnect = false;
    this.cleanup();
    if (this.socket) {
      this.sendClose();
      this.socket.destroy();
      this.socket = null;
    }
    this.connected = false;
  }

  /** Subscribe to a topic */
  subscribe(topic: string, requireAck: boolean = true): bigint {
    const topicBuffer = Buffer.from(topic, "utf-8");
    const flags = requireAck ? Flags.ACK_REQUIRED : Flags.NONE;
    const msgId = this.sendFrame(FrameType.SUBSCRIBE, topicBuffer, flags);
    return msgId;
  }

  /** Unsubscribe from a topic */
  unsubscribe(topic: string, requireAck: boolean = true): bigint {
    const topicBuffer = Buffer.from(topic, "utf-8");
    const flags = requireAck ? Flags.ACK_REQUIRED : Flags.NONE;
    const msgId = this.sendFrame(FrameType.UNSUBSCRIBE, topicBuffer, flags);
    return msgId;
  }

  /** Publish a message to a topic */
  publish(
    topic: string,
    message: string | Buffer,
    flags: number = Flags.NONE
  ): bigint {
    const topicBuffer = Buffer.from(topic, "utf-8");
    const messageBuffer = Buffer.isBuffer(message)
      ? message
      : Buffer.from(message, "utf-8");

    const payload = Buffer.alloc(2 + topicBuffer.length + messageBuffer.length);
    payload.writeUInt16BE(topicBuffer.length, 0);
    topicBuffer.copy(payload, 2);
    messageBuffer.copy(payload, 2 + topicBuffer.length);

    const msgId = this.sendFrame(FrameType.PUBLISH, payload, flags);
    return msgId;
  }

  /** Send a ping to the server */
  ping(): bigint {
    return this.sendFrame(FrameType.PING, Buffer.alloc(0));
  }

  /** Send raw frame (advanced usage) */
  sendRawFrame(
    frameType: FrameTypeValue,
    payload: Buffer,
    flags: number = 0
  ): bigint {
    return this.sendFrame(frameType, payload, flags);
  }

  // --------------------------------------------------------------------------
  // Event Emitter Override for Type Safety
  // --------------------------------------------------------------------------

  override on<K extends keyof MIPClientEvents>(
    event: K,
    listener: MIPClientEvents[K]
  ): this {
    return super.on(event, listener);
  }

  override once<K extends keyof MIPClientEvents>(
    event: K,
    listener: MIPClientEvents[K]
  ): this {
    return super.once(event, listener);
  }

  override emit<K extends keyof MIPClientEvents>(
    event: K,
    ...args: Parameters<MIPClientEvents[K]>
  ): boolean {
    return super.emit(event, ...args);
  }

  override off<K extends keyof MIPClientEvents>(
    event: K,
    listener: MIPClientEvents[K]
  ): this {
    return super.off(event, listener);
  }

  // --------------------------------------------------------------------------
  // Private Methods
  // --------------------------------------------------------------------------

  private generateMsgId(): bigint {
    this.msgIdCounter++;
    return BigInt(Date.now()) * 1000000n + this.msgIdCounter;
  }

  private buildHeader(
    frameType: FrameTypeValue,
    payloadLength: number,
    flags: number = 0,
    msgId?: bigint
  ): Buffer {
    const header = Buffer.alloc(HEADER_SIZE);
    let offset = 0;

    const id = msgId ?? this.generateMsgId();

    header.writeUInt32BE(MAGIC, offset);
    offset += 4;
    header.writeUInt8(VERSION, offset);
    offset += 1;
    header.writeUInt8(flags, offset);
    offset += 1;
    header.writeUInt16BE(frameType, offset);
    offset += 2;
    header.writeUInt16BE(MSG_KIND_EVENT, offset);
    offset += 2;
    header.writeUInt16BE(0, offset);
    offset += 2; // reserved
    header.writeUInt32BE(payloadLength, offset);
    offset += 4;
    header.writeBigUInt64BE(id, offset);

    return header;
  }

  private sendFrame(
    frameType: FrameTypeValue,
    payload: Buffer,
    flags: number = 0
  ): bigint {
    if (!this.socket || !this.connected) {
      throw new Error("Client is not connected");
    }

    const msgId = this.generateMsgId();
    const header = this.buildHeader(frameType, payload.length, flags, msgId);
    this.socket.write(Buffer.concat([header, payload]));
    return msgId;
  }

  private sendClose(): void {
    if (this.socket && this.connected) {
      try {
        const header = this.buildHeader(FrameType.CLOSE, 0);
        this.socket.write(header);
      } catch {
        // Ignore errors during close
      }
    }
  }

  private handleData(data: Buffer): void {
    this.buffer = Buffer.concat([this.buffer, data]);

    while (this.buffer.length >= HEADER_SIZE) {
      const magic = this.buffer.readUInt32BE(0);

      if (magic !== MAGIC) {
        this.emitError({ message: "Invalid magic number", code: magic });
        this.socket?.destroy();
        return;
      }

      const payloadLength = this.buffer.readUInt32BE(12);

      if (this.buffer.length < HEADER_SIZE + payloadLength) {
        return; // Wait for more data
      }

      const header = this.parseHeader(this.buffer.subarray(0, HEADER_SIZE));
      const payload = this.buffer.subarray(
        HEADER_SIZE,
        HEADER_SIZE + payloadLength
      );

      this.processFrame(header, payload);

      this.buffer = this.buffer.subarray(HEADER_SIZE + payloadLength);
    }
  }

  private parseHeader(buffer: Buffer): FrameHeader {
    return {
      magic: buffer.readUInt32BE(0),
      version: buffer.readUInt8(4),
      flags: buffer.readUInt8(5),
      frameType: buffer.readUInt16BE(6) as FrameTypeValue,
      msgKind: buffer.readUInt16BE(8),
      payloadLength: buffer.readUInt32BE(12),
      msgId: buffer.readBigUInt64BE(16),
    };
  }

  private processFrame(header: FrameHeader, payload: Buffer): void {
    // Emit raw frame event for advanced usage
    this.emit("frame", header, payload);

    switch (header.frameType) {
      case FrameType.EVENT:
      case FrameType.PUBLISH: {
        const msg = this.parseMessage(header, payload);
        if (msg) {
          if (header.frameType === FrameType.EVENT) {
            this.emit("event", msg);
          }
          this.emit("message", msg);
        }
        break;
      }

      case FrameType.ACK:
        this.emit("ack", header.msgId);
        break;

      case FrameType.PONG:
        this.emit("pong");
        break;

      case FrameType.ERROR: {
        const errorMsg = payload.toString("utf-8");
        this.emitError({ message: errorMsg, raw: payload });
        break;
      }

      case FrameType.CLOSE:
        this.handleClose();
        break;
    }
  }

  private parseMessage(header: FrameHeader, payload: Buffer): MIPMessage | null {
    if (payload.length < 2) {
      return null;
    }

    const topicLength = payload.readUInt16BE(0);

    if (payload.length < 2 + topicLength) {
      return null;
    }

    const topic = payload.subarray(2, 2 + topicLength).toString("utf-8");
    const message = payload.subarray(2 + topicLength).toString("utf-8");

    return { topic, message, header };
  }

  private handleClose(): void {
    const wasConnected = this.connected;
    this.connected = false;
    this.cleanup();

    if (wasConnected) {
      this.emit("disconnect");
    }

    if (this.options.autoReconnect) {
      this.scheduleReconnect();
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) {
      return;
    }

    const maxAttempts = this.options.maxReconnectAttempts;
    if (maxAttempts > 0 && this.reconnectAttempts >= maxAttempts) {
      this.emitError({
        message: `Max reconnection attempts (${maxAttempts}) reached`,
      });
      return;
    }

    this.reconnectAttempts++;
    this.emit("reconnecting", this.reconnectAttempts);

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect().catch(() => {
        // Error already emitted, will retry if autoReconnect is still true
        if (this.options.autoReconnect) {
          this.scheduleReconnect();
        }
      });
    }, this.options.reconnectDelay);
  }

  private setupPingInterval(): void {
    if (this.options.pingInterval > 0) {
      this.pingTimer = setInterval(() => {
        if (this.connected) {
          this.ping();
        }
      }, this.options.pingInterval);
    }
  }

  private cleanup(): void {
    if (this.pingTimer) {
      clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private emitError(error: MIPError): void {
    this.emit("error", error);
  }
}

// ============================================================================
// Utility Functions
// ============================================================================

/** Get the name of a frame type */
export function getFrameTypeName(type: FrameTypeValue): string {
  const entry = Object.entries(FrameType).find(([, value]) => value === type);
  return entry ? entry[0] : "UNKNOWN";
}

/** Create a client with default options */
export function createClient(
  host: string = "127.0.0.1",
  port: number = 9000,
  options: Partial<MIPClientOptions> = {}
): MIPClient {
  return new MIPClient({ host, port, ...options });
}

export default MIPClient;
