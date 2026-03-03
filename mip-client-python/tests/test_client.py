"""Tests for MIP Client Python"""

import pytest
import struct
from mip_client import (
    MIPClient,
    MIPClientOptions,
    FrameType,
    Flags,
    FrameHeader,
    MIPMessage,
    MIPError,
    create_client,
    get_frame_type_name,
    MAGIC,
    VERSION,
    HEADER_SIZE,
)


class TestConstants:
    """Test protocol constants"""

    def test_magic_number(self):
        assert MAGIC == 0x4D534950  # "MSIP"

    def test_version(self):
        assert VERSION == 1

    def test_header_size(self):
        assert HEADER_SIZE == 24


class TestFrameType:
    """Test FrameType enum"""

    def test_frame_types_exist(self):
        assert FrameType.HELLO == 0x0001
        assert FrameType.SUBSCRIBE == 0x0002
        assert FrameType.UNSUBSCRIBE == 0x0003
        assert FrameType.PUBLISH == 0x0004
        assert FrameType.EVENT == 0x0005
        assert FrameType.ACK == 0x0006
        assert FrameType.ERROR == 0x0007
        assert FrameType.PING == 0x0008
        assert FrameType.PONG == 0x0009
        assert FrameType.CLOSE == 0x000A

    def test_get_frame_type_name(self):
        assert get_frame_type_name(FrameType.HELLO) == "HELLO"
        assert get_frame_type_name(FrameType.SUBSCRIBE) == "SUBSCRIBE"
        assert get_frame_type_name(FrameType.PUBLISH) == "PUBLISH"


class TestFlags:
    """Test Flags enum"""

    def test_flags_values(self):
        assert Flags.NONE == 0b0000_0000
        assert Flags.ACK_REQUIRED == 0b0000_0001
        assert Flags.COMPRESSED == 0b0000_0010
        assert Flags.URGENT == 0b0000_0100

    def test_flags_combination(self):
        combined = Flags.ACK_REQUIRED | Flags.URGENT
        assert combined == 0b0000_0101


class TestMIPClientOptions:
    """Test MIPClientOptions dataclass"""

    def test_default_values(self):
        options = MIPClientOptions()
        assert options.host == "127.0.0.1"
        assert options.port == 9000
        assert options.auto_reconnect is True
        assert options.reconnect_delay == pytest.approx(3.0)
        assert options.max_reconnect_attempts == 10
        assert options.ping_interval == pytest.approx(0.0)

    def test_custom_values(self):
        options = MIPClientOptions(
            host="192.168.1.1",
            port=8080,
            auto_reconnect=False,
            reconnect_delay=5.0,
            max_reconnect_attempts=5,
            ping_interval=10.0,
        )
        assert options.host == "192.168.1.1"
        assert options.port == 8080
        assert options.auto_reconnect is False
        assert options.reconnect_delay == pytest.approx(5.0)
        assert options.max_reconnect_attempts == 5
        assert options.ping_interval == pytest.approx(10.0)


class TestMIPClient:
    """Test MIPClient class"""

    def test_client_creation_with_options(self):
        options = MIPClientOptions(host="localhost", port=9001)
        client = MIPClient(options)
        assert client.options.host == "localhost"
        assert client.options.port == 9001

    def test_client_creation_with_kwargs(self):
        client = MIPClient(host="localhost", port=9001)
        assert client.options.host == "localhost"
        assert client.options.port == 9001

    def test_is_connected_initially_false(self):
        client = MIPClient()
        assert client.is_connected is False

    def test_event_callback_registration(self):
        client = MIPClient()
        callback_called = []

        client.on_connect(lambda: callback_called.append("connect"))
        client.on_disconnect(lambda: callback_called.append("disconnect"))
        client.on_message(lambda msg: callback_called.append("message"))
        client.on_error(lambda err: callback_called.append("error"))

        # Verify callbacks are registered (internal state)
        assert len(client._on_connect) == 1
        assert len(client._on_disconnect) == 1
        assert len(client._on_message) == 1
        assert len(client._on_error) == 1

    def test_generate_msg_id(self):
        client = MIPClient()
        id1 = client._generate_msg_id()
        id2 = client._generate_msg_id()
        assert id1 != id2
        assert id2 > id1

    def test_build_header(self):
        client = MIPClient()
        header = client._build_header(FrameType.PUBLISH, 100, Flags.ACK_REQUIRED, 12345)
        
        assert len(header) == HEADER_SIZE
        
        # Parse header
        magic = struct.unpack(">I", header[0:4])[0]
        version = header[4]
        flags = header[5]
        frame_type = struct.unpack(">H", header[6:8])[0]
        payload_length = struct.unpack(">I", header[12:16])[0]
        msg_id = struct.unpack(">Q", header[16:24])[0]
        
        assert magic == MAGIC
        assert version == VERSION
        assert flags == Flags.ACK_REQUIRED
        assert frame_type == FrameType.PUBLISH
        assert payload_length == 100
        assert msg_id == 12345

    def test_parse_header(self):
        client = MIPClient()
        
        # Build a valid header
        original_header = client._build_header(FrameType.SUBSCRIBE, 50, Flags.URGENT, 99999)
        parsed = client._parse_header(original_header)
        
        assert parsed.magic == MAGIC
        assert parsed.version == VERSION
        assert parsed.flags == Flags.URGENT
        assert parsed.frame_type == FrameType.SUBSCRIBE
        assert parsed.payload_length == 50
        assert parsed.msg_id == 99999


class TestCreateClient:
    """Test create_client helper function"""

    def test_create_client_defaults(self):
        client = create_client()
        assert client.options.host == "127.0.0.1"
        assert client.options.port == 9000

    def test_create_client_custom(self):
        client = create_client("192.168.1.1", 8080, auto_reconnect=False)
        assert client.options.host == "192.168.1.1"
        assert client.options.port == 8080
        assert client.options.auto_reconnect is False


class TestMIPError:
    """Test MIPError class"""

    def test_error_message(self):
        error = MIPError(message="Connection failed")
        assert str(error) == "Connection failed"

    def test_error_with_code(self):
        error = MIPError(message="Invalid magic", code=12345)
        assert error.code == 12345

    def test_error_with_raw(self):
        raw_data = b"raw error data"
        error = MIPError(message="Error", raw=raw_data)
        assert error.raw == raw_data


class TestMIPMessage:
    """Test MIPMessage dataclass"""

    def test_message_creation(self):
        header = FrameHeader(
            magic=MAGIC,
            version=VERSION,
            flags=0,
            frame_type=FrameType.EVENT,
            msg_kind=1,
            payload_length=10,
            msg_id=12345,
        )
        msg = MIPMessage(header=header, topic="test-topic", message="Hello!")
        
        assert msg.topic == "test-topic"
        assert msg.message == "Hello!"
        assert msg.header.frame_type == FrameType.EVENT
