import {
  describe,
  it,
  expect,
  beforeEach,
  afterEach,
  jest,
} from "@jest/globals";
import { EventEmitter } from "events";

// ============================================================================
// Mock Socket Class
// ============================================================================

let shouldFailConnection = false;

class MockSocket extends EventEmitter {
  writtenData: Buffer[] = [];
  destroyed = false;

  connect(_port: number, _host: string, callback?: () => void): this {
    setImmediate(() => {
      if (shouldFailConnection) {
        this.emit("error", new Error("Connection refused"));
        shouldFailConnection = false; // Reset for next test
      } else {
        this.emit("connect");
        callback?.();
      }
    });
    return this;
  }

  write(data: Buffer): boolean {
    this.writtenData.push(Buffer.from(data));
    return true;
  }

  destroy(): void {
    this.destroyed = true;
    setImmediate(() => this.emit("close"));
  }
}

let mockSocketInstance: MockSocket;

// Mock the net module BEFORE importing MIPClient
jest.unstable_mockModule("net", () => ({
  Socket: jest.fn(() => {
    mockSocketInstance = new MockSocket();
    return mockSocketInstance;
  }),
}));

// Import after mocking
const {
  MIPClient,
  FrameType,
  Flags,
  getFrameTypeName,
  createClient,
} = await import("./index.js");

// ============================================================================
// Test Helpers
// ============================================================================

const MAGIC = 0x4d534950;
const VERSION = 1;

function buildMockFrame(
  frameType: number,
  payload: Buffer = Buffer.alloc(0),
  flags: number = 0,
  msgId: bigint = 12345n,
): Buffer {
  const header = Buffer.alloc(24);
  let offset = 0;

  header.writeUInt32BE(MAGIC, offset);
  offset += 4;
  header.writeUInt8(VERSION, offset);
  offset += 1;
  header.writeUInt8(flags, offset);
  offset += 1;
  header.writeUInt16BE(frameType, offset);
  offset += 2;
  header.writeUInt16BE(0x0001, offset);
  offset += 2;
  header.writeUInt16BE(0, offset);
  offset += 2;
  header.writeUInt32BE(payload.length, offset);
  offset += 4;
  header.writeBigUInt64BE(msgId, offset);

  return Buffer.concat([header, payload]);
}

function buildMessagePayload(topic: string, message: string): Buffer {
  const topicBuffer = Buffer.from(topic, "utf-8");
  const messageBuffer = Buffer.from(message, "utf-8");
  const payload = Buffer.alloc(2 + topicBuffer.length + messageBuffer.length);
  payload.writeUInt16BE(topicBuffer.length, 0);
  topicBuffer.copy(payload, 2);
  messageBuffer.copy(payload, 2 + topicBuffer.length);
  return payload;
}

// ============================================================================
// Tests
// ============================================================================

describe("MIPClient", () => {
  let client: InstanceType<typeof MIPClient>;

  beforeEach(() => {
    shouldFailConnection = false; // Reset flag for each test
    client = new MIPClient({
      host: "127.0.0.1",
      port: 9000,
      autoReconnect: false,
    });
  });

  afterEach(() => {
    shouldFailConnection = false; // Ensure flag is reset
    client.disconnect();
    jest.clearAllMocks();
  });

  describe("Connection", () => {
    it("should connect successfully", async () => {
      await client.connect();
      expect(client.isConnected).toBe(true);
    });

    it("should emit connect event", async () => {
      const connectHandler = jest.fn();
      client.on("connect", connectHandler);

      await client.connect();

      expect(connectHandler).toHaveBeenCalledTimes(1);
    });

    it("should emit disconnect event when closed", async () => {
      const disconnectHandler = jest.fn();
      client.on("disconnect", disconnectHandler);

      await client.connect();
      mockSocketInstance.emit("close");

      expect(disconnectHandler).toHaveBeenCalledTimes(1);
    });

    it("should handle connection errors", async () => {
      // Set flag to make next connection fail
      shouldFailConnection = true;

      const errorClient = new MIPClient({
        host: "127.0.0.1",
        port: 9000,
        autoReconnect: false,
      });

      // Add error handler to prevent unhandled error
      errorClient.on("error", () => {});

      await expect(errorClient.connect()).rejects.toThrow("Connection refused");
    });
  });

  describe("Subscribe", () => {
    it("should send SUBSCRIBE frame", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.subscribe("test-topic");

      expect(mockSocketInstance.writtenData.length).toBe(1);
      const frame = mockSocketInstance.writtenData[0]!;
      const frameType = frame.readUInt16BE(6);
      expect(frameType).toBe(FrameType.SUBSCRIBE);
    });

    it("should include topic in payload", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.subscribe("my-topic");

      const frame = mockSocketInstance.writtenData[0]!;
      const payload = frame.subarray(24).toString("utf-8");
      expect(payload).toBe("my-topic");
    });

    it("should set ACK_REQUIRED flag by default", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.subscribe("topic");

      const frame = mockSocketInstance.writtenData[0]!;
      const flags = frame.readUInt8(5);
      expect(flags & Flags.ACK_REQUIRED).toBeTruthy();
    });

    it("should allow disabling ACK_REQUIRED", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.subscribe("topic", false);

      const frame = mockSocketInstance.writtenData[0]!;
      const flags = frame.readUInt8(5);
      expect(flags & Flags.ACK_REQUIRED).toBeFalsy();
    });
  });

  describe("Unsubscribe", () => {
    it("should send UNSUBSCRIBE frame", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.unsubscribe("test-topic");

      const frame = mockSocketInstance.writtenData[0]!;
      const frameType = frame.readUInt16BE(6);
      expect(frameType).toBe(FrameType.UNSUBSCRIBE);
    });
  });

  describe("Publish", () => {
    it("should send PUBLISH frame", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.publish("topic", "message");

      const frame = mockSocketInstance.writtenData[0]!;
      const frameType = frame.readUInt16BE(6);
      expect(frameType).toBe(FrameType.PUBLISH);
    });

    it("should encode topic and message correctly", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.publish("hello", "world");

      const frame = mockSocketInstance.writtenData[0]!;
      const payload = frame.subarray(24);
      const topicLen = payload.readUInt16BE(0);
      const topic = payload.subarray(2, 2 + topicLen).toString("utf-8");
      const message = payload.subarray(2 + topicLen).toString("utf-8");

      expect(topic).toBe("hello");
      expect(message).toBe("world");
    });

    it("should accept Buffer as message", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      const binaryData = Buffer.from([0x01, 0x02, 0x03]);
      client.publish("topic", binaryData);

      const frame = mockSocketInstance.writtenData[0]!;
      const payload = frame.subarray(24);
      const topicLen = payload.readUInt16BE(0);
      const messageBytes = payload.subarray(2 + topicLen);

      expect(messageBytes).toEqual(binaryData);
    });

    it("should apply custom flags", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.publish("topic", "msg", Flags.URGENT | Flags.COMPRESSED);

      const frame = mockSocketInstance.writtenData[0]!;
      const flags = frame.readUInt8(5);
      expect(flags & Flags.URGENT).toBeTruthy();
      expect(flags & Flags.COMPRESSED).toBeTruthy();
    });
  });

  describe("Ping", () => {
    it("should send PING frame", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.ping();

      const frame = mockSocketInstance.writtenData[0]!;
      const frameType = frame.readUInt16BE(6);
      expect(frameType).toBe(FrameType.PING);
    });

    it("should have empty payload", async () => {
      await client.connect();
      mockSocketInstance.writtenData = [];

      client.ping();

      const frame = mockSocketInstance.writtenData[0]!;
      const payloadLen = frame.readUInt32BE(12);
      expect(payloadLen).toBe(0);
    });
  });

  describe("Incoming Frames", () => {
    it("should emit message event on EVENT frame", async () => {
      const messageHandler = jest.fn();
      client.on("message", messageHandler);

      await client.connect();

      const payload = buildMessagePayload("test-topic", "test-message");
      const frame = buildMockFrame(FrameType.EVENT, payload);
      mockSocketInstance.emit("data", frame);

      expect(messageHandler).toHaveBeenCalledTimes(1);
      expect(messageHandler).toHaveBeenCalledWith(
        expect.objectContaining({
          topic: "test-topic",
          message: "test-message",
        }),
      );
    });

    it("should emit event and message on EVENT frame", async () => {
      const eventHandler = jest.fn();
      const messageHandler = jest.fn();
      client.on("event", eventHandler);
      client.on("message", messageHandler);

      await client.connect();

      const payload = buildMessagePayload("topic", "msg");
      const frame = buildMockFrame(FrameType.EVENT, payload);
      mockSocketInstance.emit("data", frame);

      expect(eventHandler).toHaveBeenCalledTimes(1);
      expect(messageHandler).toHaveBeenCalledTimes(1);
    });

    it("should emit ack event on ACK frame", async () => {
      const ackHandler = jest.fn();
      client.on("ack", ackHandler);

      await client.connect();

      const frame = buildMockFrame(FrameType.ACK, Buffer.alloc(0), 0, 99999n);
      mockSocketInstance.emit("data", frame);

      expect(ackHandler).toHaveBeenCalledWith(99999n);
    });

    it("should emit pong event on PONG frame", async () => {
      const pongHandler = jest.fn();
      client.on("pong", pongHandler);

      await client.connect();

      const frame = buildMockFrame(FrameType.PONG);
      mockSocketInstance.emit("data", frame);

      expect(pongHandler).toHaveBeenCalledTimes(1);
    });

    it("should emit error event on ERROR frame", async () => {
      const errorHandler = jest.fn();
      client.on("error", errorHandler);

      await client.connect();

      const errorPayload = Buffer.from("Something went wrong", "utf-8");
      const frame = buildMockFrame(FrameType.ERROR, errorPayload);
      mockSocketInstance.emit("data", frame);

      expect(errorHandler).toHaveBeenCalledWith(
        expect.objectContaining({
          message: "Something went wrong",
        }),
      );
    });

    it("should handle fragmented data", async () => {
      const messageHandler = jest.fn();
      client.on("message", messageHandler);

      await client.connect();

      const payload = buildMessagePayload("topic", "message");
      const frame = buildMockFrame(FrameType.EVENT, payload);

      // Send in two parts
      mockSocketInstance.emit("data", frame.subarray(0, 10));
      mockSocketInstance.emit("data", frame.subarray(10));

      expect(messageHandler).toHaveBeenCalledTimes(1);
    });

    it("should handle multiple frames in one data event", async () => {
      const ackHandler = jest.fn();
      client.on("ack", ackHandler);

      await client.connect();

      const frame1 = buildMockFrame(FrameType.ACK, Buffer.alloc(0), 0, 1n);
      const frame2 = buildMockFrame(FrameType.ACK, Buffer.alloc(0), 0, 2n);
      mockSocketInstance.emit("data", Buffer.concat([frame1, frame2]));

      expect(ackHandler).toHaveBeenCalledTimes(2);
      expect(ackHandler).toHaveBeenNthCalledWith(1, 1n);
      expect(ackHandler).toHaveBeenNthCalledWith(2, 2n);
    });

    it("should reject invalid magic number", async () => {
      const errorHandler = jest.fn();
      client.on("error", errorHandler);

      await client.connect();

      const invalidFrame = Buffer.alloc(24);
      invalidFrame.writeUInt32BE(0xdeadbeef, 0);
      mockSocketInstance.emit("data", invalidFrame);

      expect(errorHandler).toHaveBeenCalledWith(
        expect.objectContaining({
          message: "Invalid magic number",
        }),
      );
      expect(mockSocketInstance.destroyed).toBe(true);
    });
  });

  describe("Reconnection", () => {
    it("should emit reconnecting event", async () => {
      const reconnectClient = new MIPClient({
        host: "127.0.0.1",
        port: 9000,
        autoReconnect: true,
        reconnectDelay: 50,
        maxReconnectAttempts: 2,
      });

      const reconnectingHandler = jest.fn();
      reconnectClient.on("reconnecting", reconnectingHandler);

      await reconnectClient.connect();
      mockSocketInstance.emit("close");

      // Wait for reconnect attempt
      await new Promise((r) => setTimeout(r, 100));

      expect(reconnectingHandler).toHaveBeenCalledWith(1);

      // Clean up
      reconnectClient.disconnect();
    });
  });

  describe("Error handling", () => {
    it("should throw when publishing without connection", () => {
      expect(() => client.publish("topic", "msg")).toThrow(
        "Client is not connected",
      );
    });

    it("should throw when subscribing without connection", () => {
      expect(() => client.subscribe("topic")).toThrow(
        "Client is not connected",
      );
    });

    it("should throw when pinging without connection", () => {
      expect(() => client.ping()).toThrow("Client is not connected");
    });
  });
});

describe("Utility Functions", () => {
  describe("getFrameTypeName", () => {
    it("should return frame type name", () => {
      expect(getFrameTypeName(FrameType.HELLO)).toBe("HELLO");
      expect(getFrameTypeName(FrameType.SUBSCRIBE)).toBe("SUBSCRIBE");
      expect(getFrameTypeName(FrameType.PUBLISH)).toBe("PUBLISH");
      expect(getFrameTypeName(FrameType.ACK)).toBe("ACK");
    });

    it("should return UNKNOWN for invalid type", () => {
      expect(getFrameTypeName(0xffff as never)).toBe("UNKNOWN");
    });
  });

  describe("createClient", () => {
    it("should create client with defaults", () => {
      const c = createClient();
      expect(c).toBeInstanceOf(MIPClient);
    });

    it("should accept custom options", () => {
      const c = createClient("192.168.1.1", 8080, { autoReconnect: false });
      expect(c).toBeInstanceOf(MIPClient);
    });
  });
});

describe("Constants", () => {
  it("should export FrameType constants", () => {
    expect(FrameType.HELLO).toBe(0x0001);
    expect(FrameType.SUBSCRIBE).toBe(0x0002);
    expect(FrameType.UNSUBSCRIBE).toBe(0x0003);
    expect(FrameType.PUBLISH).toBe(0x0004);
    expect(FrameType.EVENT).toBe(0x0005);
    expect(FrameType.ACK).toBe(0x0006);
    expect(FrameType.ERROR).toBe(0x0007);
    expect(FrameType.PING).toBe(0x0008);
    expect(FrameType.PONG).toBe(0x0009);
    expect(FrameType.CLOSE).toBe(0x000a);
  });

  it("should export Flags constants", () => {
    expect(Flags.NONE).toBe(0b0000_0000);
    expect(Flags.ACK_REQUIRED).toBe(0b0000_0001);
    expect(Flags.COMPRESSED).toBe(0b0000_0010);
    expect(Flags.URGENT).toBe(0b0000_0100);
  });
});
