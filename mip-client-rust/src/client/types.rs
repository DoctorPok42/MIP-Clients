use thiserror::Error;

use crate::protocol::header::Header;

// ============================================================================
// Error Types
// ============================================================================

/// MIP Client errors
#[derive(Debug, Error)]
pub enum MIPError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Client not connected")]
    NotConnected,

    #[error("Invalid magic number: {0}")]
    InvalidMagic(u32),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Max reconnection attempts ({0}) reached")]
    MaxReconnectAttempts(u32),

    #[error("Server error: {0}")]
    ServerError(String),
}

/// Result type for MIP operations
pub type MIPResult<T> = Result<T, MIPError>;

// ============================================================================
// Configuration Options
// ============================================================================

/// Configuration options for the MIP client
#[derive(Debug, Clone)]
pub struct MIPClientOptions {
    /// Server host address
    pub host: String,
    /// Server port number
    pub port: u16,
    /// Auto-reconnect on disconnect
    pub auto_reconnect: bool,
    /// Reconnect delay in milliseconds
    pub reconnect_delay_ms: u64,
    /// Maximum reconnection attempts (0 = infinite)
    pub max_reconnect_attempts: u32,
    /// Ping interval in milliseconds (0 = disabled)
    pub ping_interval_ms: u64,
}

impl Default for MIPClientOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9000,
            auto_reconnect: true,
            reconnect_delay_ms: 3000,
            max_reconnect_attempts: 10,
            ping_interval_ms: 0,
        }
    }
}

impl MIPClientOptions {
    /// Set the host address
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Set the port number
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set auto-reconnect option
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// Set reconnect delay in milliseconds
    pub fn reconnect_delay_ms(mut self, delay: u64) -> Self {
        self.reconnect_delay_ms = delay;
        self
    }

    /// Set maximum reconnection attempts
    pub fn max_reconnect_attempts(mut self, attempts: u32) -> Self {
        self.max_reconnect_attempts = attempts;
        self
    }

    /// Set ping interval in milliseconds
    pub fn ping_interval_ms(mut self, interval: u64) -> Self {
        self.ping_interval_ms = interval;
        self
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Received message event
#[derive(Debug, Clone)]
pub struct MIPMessage {
    /// Frame header
    pub header: Header,
    /// Topic name
    pub topic: String,
    /// Message content
    pub message: String,
}
