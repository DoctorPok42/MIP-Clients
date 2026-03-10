"""
MIP Client - Python client for the MIP (MSIP) protocol.
Handles connections, events, errors, and auto-reconnection.
"""

import asyncio
import struct
import time
from dataclasses import dataclass
from enum import IntEnum, IntFlag
from typing import Callable, Optional, Any
import logging

__version__ = "1.0.0"
__all__ = [
    "MIPClient",
    "FrameType",
    "Flags",
    "FrameHeader",
    "MIPMessage",
    "MIPError",
    "MIPClientOptions",
    "create_client",
    "get_frame_type_name",
    "MAGIC",
    "VERSION",
    "HEADER_SIZE",
]

logger = logging.getLogger(__name__)

# ============================================================================
# Constants
# ============================================================================

MAGIC = 0x4D534950  # "MSIP"
VERSION = 1
HEADER_SIZE = 24
MSG_KIND_EVENT = 0x0001


class FrameType(IntEnum):
    """Frame types for the MIP protocol"""
    HELLO = 0x0001
    SUBSCRIBE = 0x0002
    UNSUBSCRIBE = 0x0003
    PUBLISH = 0x0004
    EVENT = 0x0005
    ACK = 0x0006
    ERROR = 0x0007
    PING = 0x0008
    PONG = 0x0009
    CLOSE = 0x000A


class Flags(IntFlag):
    """Flags for frame options"""
    NONE = 0b0000_0000
    ACK_REQUIRED = 0b0000_0001
    COMPRESSED = 0b0000_0010
    URGENT = 0b0000_0100


# ============================================================================
# Types & Interfaces
# ============================================================================

@dataclass
class MIPClientOptions:
    """Configuration options for the MIP client"""
    host: str = "127.0.0.1"
    port: int = 9000
    client_id: str = ""
    auto_reconnect: bool = True
    reconnect_delay: float = 3.0
    max_reconnect_attempts: int = 10
    ping_interval: float = 0.0


@dataclass
class FrameHeader:
    """Parsed frame header"""
    magic: int
    version: int
    flags: int
    frame_type: FrameType
    msg_kind: int
    payload_length: int
    msg_id: int


@dataclass
class MIPMessage:
    """Received message event"""
    header: FrameHeader
    topic: str
    message: str


@dataclass
class MIPError(Exception):
    """Error details"""
    message: str
    code: Optional[int] = None
    raw: Optional[bytes] = None

    def __str__(self) -> str:
        return self.message


# ============================================================================
# Event Callback Types
# ============================================================================

OnConnect = Callable[[], None]
OnDisconnect = Callable[[], None]
OnReconnecting = Callable[[int], None]
OnMessage = Callable[[MIPMessage], None]
OnEvent = Callable[[MIPMessage], None]
OnAck = Callable[[int], None]
OnPong = Callable[[], None]
OnError = Callable[[MIPError], None]
OnFrame = Callable[[FrameHeader, bytes], None]


# ============================================================================
# MIP Client Class
# ============================================================================

class MIPClient:
    """Async MIP protocol client with auto-reconnection support"""

    def __init__(self, options: Optional[MIPClientOptions] = None, **kwargs: Any):
        """
        Initialize MIP client.

        Args:
            options: MIPClientOptions instance
            **kwargs: Alternative way to pass options (host, port, auto_reconnect, etc...)
        """
        if options is None:
            options = MIPClientOptions(**kwargs)

        self._options = options
        self._client_id: str = options.client_id
        self._reader: Optional[asyncio.StreamReader] = None
        self._writer: Optional[asyncio.StreamWriter] = None
        self._buffer: bytes = b""
        self._connected: bool = False
        self._reconnect_attempts: int = 0
        self._reconnect_task: Optional[asyncio.Task] = None
        self._ping_task: Optional[asyncio.Task] = None
        self._read_task: Optional[asyncio.Task] = None
        self._close_task: Optional[asyncio.Task] = None
        self._msg_id_counter: int = 0
        self._running: bool = False

        # Event callbacks
        self._on_connect: list[OnConnect] = []
        self._on_disconnect: list[OnDisconnect] = []
        self._on_reconnecting: list[OnReconnecting] = []
        self._on_message: list[OnMessage] = []
        self._on_event: list[OnEvent] = []
        self._on_ack: list[OnAck] = []
        self._on_pong: list[OnPong] = []
        self._on_error: list[OnError] = []
        self._on_frame: list[OnFrame] = []

    # --------------------------------------------------------------------------
    # Properties
    # --------------------------------------------------------------------------

    @property
    def is_connected(self) -> bool:
        """Check if the client is connected"""
        return self._connected

    @property
    def options(self) -> MIPClientOptions:
        """Get client options"""
        return self._options

    # --------------------------------------------------------------------------
    # Event Registration
    # --------------------------------------------------------------------------

    def on_connect(self, callback: OnConnect) -> "MIPClient":
        """Register connect event callback"""
        self._on_connect.append(callback)
        return self

    def on_disconnect(self, callback: OnDisconnect) -> "MIPClient":
        """Register disconnect event callback"""
        self._on_disconnect.append(callback)
        return self

    def on_reconnecting(self, callback: OnReconnecting) -> "MIPClient":
        """Register reconnecting event callback"""
        self._on_reconnecting.append(callback)
        return self

    def on_message(self, callback: OnMessage) -> "MIPClient":
        """Register message event callback"""
        self._on_message.append(callback)
        return self

    def on_event(self, callback: OnEvent) -> "MIPClient":
        """Register event callback"""
        self._on_event.append(callback)
        return self

    def on_ack(self, callback: OnAck) -> "MIPClient":
        """Register ACK event callback"""
        self._on_ack.append(callback)
        return self

    def on_pong(self, callback: OnPong) -> "MIPClient":
        """Register pong event callback"""
        self._on_pong.append(callback)
        return self

    def on_error(self, callback: OnError) -> "MIPClient":
        """Register error event callback"""
        self._on_error.append(callback)
        return self

    def on_frame(self, callback: OnFrame) -> "MIPClient":
        """Register raw frame event callback"""
        self._on_frame.append(callback)
        return self

    # --------------------------------------------------------------------------
    # Public API
    # --------------------------------------------------------------------------

    async def connect(self) -> None:
        """Connect to the MIP server"""
        if self._connected:
            return

        self._running = True

        try:
            self._reader, self._writer = await asyncio.open_connection(
                self._options.host, self._options.port
            )
            self._connected = True
            self._reconnect_attempts = 0
            self._buffer = b""

            # Start background tasks
            self._read_task = asyncio.create_task(self._read_loop())
            self._setup_ping_interval()

            # Emit connect event
            for callback in self._on_connect:
                callback()

            payload = self._client_id.encode("utf-8")
            try:
                msg_id = self._send_frame(FrameType.HELLO, payload, Flags.NONE)
                self._client_id = str(msg_id)
            except Exception as e:
                logger.error("Failed to send HELLO frame: %s", e)
                self._connected = False
                self._running = False
                raise

        except Exception as e:
            self._emit_error(MIPError(message=str(e)))
            raise

    async def disconnect(self) -> None:
        """Disconnect from the server"""
        self._options.auto_reconnect = False
        self._running = False
        await self._cleanup()

        if self._writer:
            await self._send_close()
            self._writer.close()
            try:
                await self._writer.wait_closed()
            except Exception:
                pass
            self._writer = None
            self._reader = None

        self._connected = False

    def subscribe(self, topic: str, require_ack: bool = True) -> int:
        """Subscribe to a topic"""
        topic_bytes = topic.encode("utf-8")
        flags = Flags.ACK_REQUIRED if require_ack else Flags.NONE
        return self._send_frame(FrameType.SUBSCRIBE, topic_bytes, flags)

    def unsubscribe(self, topic: str, require_ack: bool = True) -> int:
        """Unsubscribe from a topic"""
        topic_bytes = topic.encode("utf-8")
        flags = Flags.ACK_REQUIRED if require_ack else Flags.NONE
        return self._send_frame(FrameType.UNSUBSCRIBE, topic_bytes, flags)

    def publish(
        self,
        topic: str,
        message: "str | bytes",
        flags: Flags = Flags.NONE
    ) -> int:
        """Publish a message to a topic"""
        topic_bytes = topic.encode("utf-8")
        if isinstance(message, bytes):
            message_bytes = message
        else:
            message_bytes = str(message).encode("utf-8")

        # Build payload: [topic_length (2 bytes)] [topic] [message]
        payload = struct.pack(">H", len(topic_bytes)) + topic_bytes + message_bytes
        return self._send_frame(FrameType.PUBLISH, payload, flags)

    def ping(self) -> int:
        """Send a ping to the server"""
        return self._send_frame(FrameType.PING, b"")

    def send_raw_frame(
        self,
        frame_type: FrameType,
        payload: bytes,
        flags: Flags = Flags.NONE
    ) -> int:
        """Send raw frame (advanced usage)"""
        return self._send_frame(frame_type, payload, flags)

    # --------------------------------------------------------------------------
    # Private Methods
    # --------------------------------------------------------------------------

    def _generate_msg_id(self) -> int:
        """Generate unique message ID"""
        self._msg_id_counter += 1
        return int(time.time() * 1000000) + self._msg_id_counter

    def _build_header(
        self,
        frame_type: FrameType,
        payload_length: int,
        flags: int = 0,
        msg_id: Optional[int] = None
    ) -> bytes:
        """Build frame header"""
        if msg_id is None:
            msg_id = self._generate_msg_id()

        # Header format: magic(4) + version(1) + flags(1) + frame_type(2) +
        #                msg_kind(2) + reserved(2) + payload_length(4) + msg_id(8)
        return struct.pack(
            ">IBBHHHI Q",
            MAGIC,
            VERSION,
            flags,
            frame_type,
            MSG_KIND_EVENT,
            0,  # reserved
            payload_length,
            msg_id
        )

    def _send_frame(
        self,
        frame_type: FrameType,
        payload: bytes,
        flags: int = 0
    ) -> int:
        """Send a frame to the server"""
        if not self._writer or not self._connected:
            raise MIPError(message="Client is not connected")

        msg_id = self._generate_msg_id()
        header = self._build_header(frame_type, len(payload), flags, msg_id)
        self._writer.write(header + payload)
        return msg_id

    async def _send_close(self) -> None:
        """Send close frame"""
        if self._writer and self._connected:
            try:
                header = self._build_header(FrameType.CLOSE, 0)
                self._writer.write(header)
                await self._writer.drain()
            except Exception:
                pass

    async def _read_loop(self) -> None:
        """Main read loop for incoming data"""
        try:
            while self._running and self._reader:
                data = await self._reader.read(4096)
                if not data:
                    break
                self._handle_data(data)
        except asyncio.CancelledError:
            raise
        except Exception as e:
            if self._connected:
                self._emit_error(MIPError(message=str(e)))
            await self._handle_close()

    def _handle_data(self, data: bytes) -> None:
        """Handle incoming data"""
        self._buffer += data

        while len(self._buffer) >= HEADER_SIZE:
            magic = struct.unpack(">I", self._buffer[:4])[0]

            if magic != MAGIC:
                self._emit_error(MIPError(message="Invalid magic number", code=magic))
                if self._writer:
                    self._writer.close()
                return

            payload_length = struct.unpack(">I", self._buffer[12:16])[0]

            if len(self._buffer) < HEADER_SIZE + payload_length:
                return  # Wait for more data

            header = self._parse_header(self._buffer[:HEADER_SIZE])
            payload = self._buffer[HEADER_SIZE:HEADER_SIZE + payload_length]

            self._process_frame(header, payload)

            self._buffer = self._buffer[HEADER_SIZE + payload_length:]

    def _parse_header(self, buffer: bytes) -> FrameHeader:
        """Parse frame header from bytes"""
        magic, version, flags, frame_type, msg_kind, _, payload_length, msg_id = struct.unpack(
            ">IBBHHHI Q", buffer
        )
        return FrameHeader(
            magic=magic,
            version=version,
            flags=flags,
            frame_type=FrameType(frame_type),
            msg_kind=msg_kind,
            payload_length=payload_length,
            msg_id=msg_id
        )

    def _process_frame(self, header: FrameHeader, payload: bytes) -> None:
        """Process received frame"""
        # Emit raw frame event
        for callback in self._on_frame:
            callback(header, payload)

        if header.frame_type in (FrameType.EVENT, FrameType.PUBLISH):
            msg = self._parse_message(header, payload)
            if msg:
                if header.frame_type == FrameType.EVENT:
                    for callback in self._on_event:
                        callback(msg)
                for callback in self._on_message:
                    callback(msg)

        elif header.frame_type == FrameType.ACK:
            for callback in self._on_ack:
                callback(header.msg_id)

        elif header.frame_type == FrameType.PONG:
            for callback in self._on_pong:
                callback()

        elif header.frame_type == FrameType.ERROR:
            error_msg = payload.decode("utf-8")
            self._emit_error(MIPError(message=error_msg, raw=payload))

        elif header.frame_type == FrameType.CLOSE:
            self._close_task = asyncio.create_task(self._handle_close())

    def _parse_message(self, header: FrameHeader, payload: bytes) -> Optional[MIPMessage]:
        """Parse message from payload"""
        if len(payload) < 2:
            return None

        topic_length = struct.unpack(">H", payload[:2])[0]

        if len(payload) < 2 + topic_length:
            return None

        topic = payload[2:2 + topic_length].decode("utf-8")
        message = payload[2 + topic_length:].decode("utf-8")

        return MIPMessage(header=header, topic=topic, message=message)

    async def _handle_close(self) -> None:
        """Handle connection close"""
        was_connected = self._connected
        self._connected = False
        await self._cleanup()

        if was_connected:
            for callback in self._on_disconnect:
                callback()

        if self._options.auto_reconnect and self._running:
            self._schedule_reconnect()

    def _schedule_reconnect(self) -> None:
        """Schedule reconnection attempt"""
        if self._reconnect_task and not self._reconnect_task.done():
            return

        max_attempts = self._options.max_reconnect_attempts
        if max_attempts > 0 and self._reconnect_attempts >= max_attempts:
            self._emit_error(MIPError(
                message=f"Max reconnection attempts ({max_attempts}) reached"
            ))
            return

        self._reconnect_attempts += 1
        for callback in self._on_reconnecting:
            callback(self._reconnect_attempts)

        async def reconnect() -> None:
            await asyncio.sleep(self._options.reconnect_delay)
            try:
                await self.connect()
            except Exception:
                if self._options.auto_reconnect and self._running:
                    self._schedule_reconnect()

        self._reconnect_task = asyncio.create_task(reconnect())

    def _setup_ping_interval(self) -> None:
        """Setup automatic ping interval"""
        if self._options.ping_interval > 0:
            async def ping_loop():
                while self._connected and self._running:
                    await asyncio.sleep(self._options.ping_interval)
                    if self._connected:
                        self.ping()

            self._ping_task = asyncio.create_task(ping_loop())

    async def _cleanup(self) -> None:
        """Cleanup background tasks"""
        if self._ping_task:
            self._ping_task.cancel()
            try:
                await self._ping_task
            except asyncio.CancelledError:
                raise
            self._ping_task = None

        if self._read_task:
            self._read_task.cancel()
            try:
                await self._read_task
            except asyncio.CancelledError:
                raise
            self._read_task = None

        if self._reconnect_task:
            self._reconnect_task.cancel()
            try:
                await self._reconnect_task
            except asyncio.CancelledError:
                raise
            self._reconnect_task = None

    def _emit_error(self, error: MIPError) -> None:
        """Emit error to callbacks"""
        for callback in self._on_error:
            callback(error)


# ============================================================================
# Utility Functions
# ============================================================================

def get_frame_type_name(frame_type: FrameType) -> str:
    """Get the name of a frame type"""
    return frame_type.name


def create_client(
    host: str = "127.0.0.1",
    port: int = 9000,
    **kwargs: Any
) -> MIPClient:
    """Create a client with default options"""
    options = MIPClientOptions(host=host, port=port, **kwargs)
    return MIPClient(options)
