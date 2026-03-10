mod events;
mod internals;
mod types;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info};

use crate::protocol::header::{FrameFlags, FrameType};

pub use events::{
    OnAck, OnConnect, OnDisconnect, OnError, OnEvent, OnFrame, OnMessage, OnPong, OnReconnecting,
};
pub use types::{MIPClientOptions, MIPError, MIPMessage, MIPResult};

use events::Callbacks;
use internals::{
    cleanup_tasks, send_close_internal, send_frame, spawn_ping_task, spawn_read_task,
    spawn_write_task, ClientCommand,
};

// ============================================================================
// MIP Client
// ============================================================================

/// Async MIP protocol client with auto-reconnection support
pub struct MIPClient {
    client_id: String,
    options: Arc<RwLock<MIPClientOptions>>,
    connected: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    msg_id_counter: Arc<AtomicU64>,
    reconnect_attempts: Arc<AtomicU64>,

    callbacks: Arc<RwLock<Callbacks>>,
    command_tx: Option<mpsc::Sender<ClientCommand>>,

    read_task: Option<JoinHandle<()>>,
    write_task: Option<JoinHandle<()>>,
    ping_task: Option<JoinHandle<()>>,
}

impl MIPClient {
    pub fn new(options: MIPClientOptions) -> Self {
        Self {
          client_id: options.client_id.clone(),
          options: Arc::new(RwLock::new(options)),
          connected: Arc::new(AtomicBool::new(false)),
          running: Arc::new(AtomicBool::new(false)),
          msg_id_counter: Arc::new(AtomicU64::new(0)),
          reconnect_attempts: Arc::new(AtomicU64::new(0)),
          callbacks: Arc::new(RwLock::new(Callbacks::default())),
          command_tx: None,
          read_task: None,
          write_task: None,
          ping_task: None,
        }
    }

    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    // --------------------------------------------------------------------------
    // Event Registration
    // --------------------------------------------------------------------------

    /// Register connect event callback
    pub fn on_connect<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks.write().await.on_connect.push(Arc::new(callback));
            });
        });
        self
    }

    /// Register disconnect event callback
    pub fn on_disconnect<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks
                    .write()
                    .await
                    .on_disconnect
                    .push(Arc::new(callback));
            });
        });
        self
    }

    /// Register reconnecting event callback
    pub fn on_reconnecting<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn(u32) + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks
                    .write()
                    .await
                    .on_reconnecting
                    .push(Arc::new(callback));
            });
        });
        self
    }

    /// Register message event callback
    pub fn on_message<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn(MIPMessage) + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks.write().await.on_message.push(Arc::new(callback));
            });
        });
        self
    }

    /// Register event callback
    pub fn on_event<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn(MIPMessage) + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks.write().await.on_event.push(Arc::new(callback));
            });
        });
        self
    }

    /// Register ACK event callback
    pub fn on_ack<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks.write().await.on_ack.push(Arc::new(callback));
            });
        });
        self
    }

    /// Register pong event callback
    pub fn on_pong<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks.write().await.on_pong.push(Arc::new(callback));
            });
        });
        self
    }

    /// Register error event callback
    pub fn on_error<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn(MIPError) + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks.write().await.on_error.push(Arc::new(callback));
            });
        });
        self
    }

    /// Register raw frame event callback
    pub fn on_frame<F>(&mut self, callback: F) -> &mut Self
    where
        F: Fn(crate::protocol::header::Header, Vec<u8>) + Send + Sync + 'static,
    {
        let callbacks = self.callbacks.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                callbacks.write().await.on_frame.push(Arc::new(callback));
            });
        });
        self
    }

    // --------------------------------------------------------------------------
    // Public API
    // --------------------------------------------------------------------------

    /// Connect to the MIP server
    pub async fn connect(&mut self) -> MIPResult<()> {
        if self.connected.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);

        let options = self.options.read().await;
        let addr = format!("{}:{}", options.host, options.port);
        drop(options);

        debug!("Connecting to {}", addr);

        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| MIPError::Connection(e.to_string()))?;

        let (read_half, write_half) = stream.into_split();

        // Create command channel
        let (command_tx, command_rx) = mpsc::channel::<ClientCommand>(100);
        self.command_tx = Some(command_tx);

        self.connected.store(true, Ordering::SeqCst);
        self.reconnect_attempts.store(0, Ordering::SeqCst);

        // Start read task
        let read_task = spawn_read_task(
            read_half,
            self.connected.clone(),
            self.running.clone(),
            self.callbacks.clone(),
            self.options.clone(),
            self.reconnect_attempts.clone(),
        );
        self.read_task = Some(read_task);

        // Start write task
        let write_task = spawn_write_task(write_half, command_rx);
        self.write_task = Some(write_task);

        // Setup ping interval
        let options = self.options.read().await;
        let ping_interval = options.ping_interval_ms;
        drop(options);

        self.ping_task = spawn_ping_task(
            ping_interval,
            self.connected.clone(),
            self.running.clone(),
            self.command_tx.clone(),
            self.msg_id_counter.clone(),
        );

        // Emit connect event
        let callbacks = self.callbacks.read().await;
        for callback in &callbacks.on_connect {
            callback();
        }

        let client_id = self.options.read().await.client_id.clone();
        let payload = client_id.as_bytes();

        if let Some(tx) = &self.command_tx {
            let res = send_frame(
              Some(tx),
              &self.connected.clone(),
              &self.msg_id_counter.clone(),
              FrameType::Hello, payload,
              FrameFlags::NONE
            );

            if let Err(e) = res {
              println!("Failed to send HELLO frame: {}", e);
                self.connected.store(false, Ordering::SeqCst);
                self.running.store(false, Ordering::SeqCst);
                return Err(e);
            } else {
              self.client_id = res.unwrap().to_string();
            }
        }

        info!("Connected to {}", addr);
        Ok(())
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self) -> MIPResult<()> {
        {
            let mut options = self.options.write().await;
            options.auto_reconnect = false;
        }

        self.running.store(false, Ordering::SeqCst);

        // Send close frame
        if let Some(tx) = &self.command_tx {
            let _ = send_close_internal(tx, &self.connected, &self.msg_id_counter).await;
            let _ = tx.send(ClientCommand::Disconnect).await;
        }

        cleanup_tasks(&mut self.ping_task, &mut self.read_task, &mut self.write_task).await;
        self.connected.store(false, Ordering::SeqCst);
        self.command_tx = None;

        info!("Disconnected");
        Ok(())
    }

    /// Subscribe to a topic
    pub fn subscribe(&self, topic: &str, require_ack: bool) -> MIPResult<u64> {
        let topic_bytes = topic.as_bytes();
        let flags = if require_ack {
            FrameFlags::ACK_REQUIRED
        } else {
            FrameFlags::NONE
        };
        send_frame(
            self.command_tx.as_ref(),
            &self.connected,
            &self.msg_id_counter,
            FrameType::Subscribe,
            topic_bytes,
            flags,
        )
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&self, topic: &str, require_ack: bool) -> MIPResult<u64> {
        let topic_bytes = topic.as_bytes();
        let flags = if require_ack {
            FrameFlags::ACK_REQUIRED
        } else {
            FrameFlags::NONE
        };
        send_frame(
            self.command_tx.as_ref(),
            &self.connected,
            &self.msg_id_counter,
            FrameType::Unsubscribe,
            topic_bytes,
            flags,
        )
    }

    /// Publish a message to a topic
    pub fn publish(&self, topic: &str, message: &str, flags: FrameFlags) -> MIPResult<u64> {
        let topic_bytes = topic.as_bytes();
        let message_bytes = message.as_bytes();

        // Build payload: [topic_length (2 bytes)] [topic] [message]
        let mut payload = Vec::with_capacity(2 + topic_bytes.len() + message_bytes.len());
        payload.extend_from_slice(&(topic_bytes.len() as u16).to_be_bytes());
        payload.extend_from_slice(topic_bytes);
        payload.extend_from_slice(message_bytes);

        send_frame(
            self.command_tx.as_ref(),
            &self.connected,
            &self.msg_id_counter,
            FrameType::Publish,
            &payload,
            flags,
        )
    }

    /// Send a ping to the server
    pub fn ping(&self) -> MIPResult<u64> {
        send_frame(
            self.command_tx.as_ref(),
            &self.connected,
            &self.msg_id_counter,
            FrameType::Ping,
            &[],
            FrameFlags::NONE,
        )
    }

    /// Send raw frame (advanced usage)
    pub fn send_raw_frame(
        &self,
        frame_type: FrameType,
        payload: &[u8],
        flags: FrameFlags,
    ) -> MIPResult<u64> {
        send_frame(
            self.command_tx.as_ref(),
            &self.connected,
            &self.msg_id_counter,
            frame_type,
            payload,
            flags,
        )
    }
}

impl Drop for MIPClient {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get the name of a frame type
pub fn get_frame_type_name(frame_type: FrameType) -> &'static str {
    match frame_type {
        FrameType::Hello => "HELLO",
        FrameType::Subscribe => "SUBSCRIBE",
        FrameType::Unsubscribe => "UNSUBSCRIBE",
        FrameType::Publish => "PUBLISH",
        FrameType::Event => "EVENT",
        FrameType::Ack => "ACK",
        FrameType::Error => "ERROR",
        FrameType::Ping => "PING",
        FrameType::Pong => "PONG",
        FrameType::Close => "CLOSE",
    }
}

/// Create a new MIP client with default options
pub fn create_client() -> MIPClient {
    MIPClient::new(MIPClientOptions::default())
}
