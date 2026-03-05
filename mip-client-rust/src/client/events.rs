use std::sync::Arc;

use crate::protocol::header::Header;
use super::types::{MIPError, MIPMessage};

// ============================================================================
// Callback Type Aliases
// ============================================================================

/// Callback invoked when connected to the server
pub type OnConnect = Arc<dyn Fn() + Send + Sync>;

/// Callback invoked when disconnected from the server
pub type OnDisconnect = Arc<dyn Fn() + Send + Sync>;

/// Callback invoked on reconnection attempt (receives attempt number)
pub type OnReconnecting = Arc<dyn Fn(u32) + Send + Sync>;

/// Callback invoked when a message is received
pub type OnMessage = Arc<dyn Fn(MIPMessage) + Send + Sync>;

/// Callback invoked when an event frame is received
pub type OnEvent = Arc<dyn Fn(MIPMessage) + Send + Sync>;

/// Callback invoked when an ACK is received (receives message ID)
pub type OnAck = Arc<dyn Fn(u64) + Send + Sync>;

/// Callback invoked when a PONG is received
pub type OnPong = Arc<dyn Fn() + Send + Sync>;

/// Callback invoked on error
pub type OnError = Arc<dyn Fn(MIPError) + Send + Sync>;

/// Callback invoked for every raw frame (receives header and payload)
pub type OnFrame = Arc<dyn Fn(Header, Vec<u8>) + Send + Sync>;

// ============================================================================
// Callbacks Container
// ============================================================================

/// Container for all registered callbacks
pub(crate) struct Callbacks {
    pub on_connect: Vec<OnConnect>,
    pub on_disconnect: Vec<OnDisconnect>,
    pub on_reconnecting: Vec<OnReconnecting>,
    pub on_message: Vec<OnMessage>,
    pub on_event: Vec<OnEvent>,
    pub on_ack: Vec<OnAck>,
    pub on_pong: Vec<OnPong>,
    pub on_error: Vec<OnError>,
    pub on_frame: Vec<OnFrame>,
}

impl Default for Callbacks {
    fn default() -> Self {
        Self {
            on_connect: Vec::new(),
            on_disconnect: Vec::new(),
            on_reconnecting: Vec::new(),
            on_message: Vec::new(),
            on_event: Vec::new(),
            on_ack: Vec::new(),
            on_pong: Vec::new(),
            on_error: Vec::new(),
            on_frame: Vec::new(),
        }
    }
}
