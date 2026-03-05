use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, warn};

use crate::protocol::header::{
    FrameFlags, FrameType, Header, MessageKind, HEADER_SIZE, MSIP_MAGIC,
};
use super::events::Callbacks;
use super::types::{MIPClientOptions, MIPError, MIPMessage, MIPResult};

// ============================================================================
// Constants
// ============================================================================

pub(crate) const MSG_KIND_EVENT: MessageKind = MessageKind::Event;

// ============================================================================
// Internal Command Types
// ============================================================================

#[derive(Debug)]
pub(crate) enum ClientCommand {
    Send(Vec<u8>),
    Disconnect,
}

// ============================================================================
// Message ID Generation
// ============================================================================

pub(crate) fn generate_msg_id(counter: &AtomicU64) -> u64 {
    let count = counter.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    timestamp + count
}

// ============================================================================
// Header Building
// ============================================================================

pub(crate) fn build_header(
    frame_type: FrameType,
    payload_length: u32,
    flags: FrameFlags,
    msg_id: u64,
) -> Header {
    Header::new(frame_type, MSG_KIND_EVENT, payload_length, msg_id, flags)
}

// ============================================================================
// Frame Sending
// ============================================================================

pub(crate) fn send_frame(
    command_tx: Option<&mpsc::Sender<ClientCommand>>,
    connected: &AtomicBool,
    msg_id_counter: &AtomicU64,
    frame_type: FrameType,
    payload: &[u8],
    flags: FrameFlags,
) -> MIPResult<u64> {
    let tx = command_tx.ok_or(MIPError::NotConnected)?;

    if !connected.load(Ordering::SeqCst) {
        return Err(MIPError::NotConnected);
    }

    let msg_id = generate_msg_id(msg_id_counter);
    let header = build_header(frame_type, payload.len() as u32, flags, msg_id);

    let mut frame = Vec::with_capacity(HEADER_SIZE + payload.len());
    frame.extend_from_slice(&header.encode());
    frame.extend_from_slice(payload);

    tx.try_send(ClientCommand::Send(frame))
        .map_err(|_| MIPError::NotConnected)?;

    Ok(msg_id)
}

pub(crate) async fn send_close_internal(
    tx: &mpsc::Sender<ClientCommand>,
    connected: &AtomicBool,
    msg_id_counter: &AtomicU64,
) -> MIPResult<()> {
    if connected.load(Ordering::SeqCst) {
        let msg_id = generate_msg_id(msg_id_counter);
        let header = build_header(FrameType::Close, 0, FrameFlags::NONE, msg_id);
        let frame = header.encode().to_vec();
        let _ = tx.send(ClientCommand::Send(frame)).await;
    }
    Ok(())
}

// ============================================================================
// Read Task
// ============================================================================

pub(crate) fn spawn_read_task(
    mut read_half: tokio::net::tcp::OwnedReadHalf,
    connected: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    callbacks: Arc<RwLock<Callbacks>>,
    options: Arc<RwLock<MIPClientOptions>>,
    reconnect_attempts: Arc<AtomicU64>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut buffer = Vec::new();

        loop {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            let mut temp_buf = [0u8; 4096];
            match read_half.read(&mut temp_buf).await {
                Ok(0) => {
                    // Connection closed
                    break;
                }
                Ok(n) => {
                    buffer.extend_from_slice(&temp_buf[..n]);
                    handle_data(&mut buffer, &callbacks).await;
                }
                Err(e) => {
                    error!("Read error: {}", e);
                    let cbs = callbacks.read().await;
                    for callback in &cbs.on_error {
                        callback(MIPError::Io(e.kind().into()));
                    }
                    break;
                }
            }
        }

        // Handle disconnection
        let was_connected = connected.swap(false, Ordering::SeqCst);

        if was_connected {
            let cbs = callbacks.read().await;
            for callback in &cbs.on_disconnect {
                callback();
            }
        }

        // Schedule reconnect if needed
        let opts = options.read().await;
        if opts.auto_reconnect && running.load(Ordering::SeqCst) {
            let max_attempts = opts.max_reconnect_attempts;
            let delay = opts.reconnect_delay_ms;
            let current_attempts = reconnect_attempts.fetch_add(1, Ordering::SeqCst) as u32 + 1;

            if max_attempts == 0 || current_attempts <= max_attempts {
                let cbs = callbacks.read().await;
                for callback in &cbs.on_reconnecting {
                    callback(current_attempts);
                }
                drop(cbs);
                drop(opts);

                warn!("Reconnecting in {}ms (attempt {})", delay, current_attempts);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            } else {
                let cbs = callbacks.read().await;
                for callback in &cbs.on_error {
                    callback(MIPError::MaxReconnectAttempts(max_attempts));
                }
            }
        }
    })
}

// ============================================================================
// Write Task
// ============================================================================

pub(crate) fn spawn_write_task(
    mut write_half: tokio::net::tcp::OwnedWriteHalf,
    mut command_rx: mpsc::Receiver<ClientCommand>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                ClientCommand::Send(data) => {
                    if let Err(e) = write_half.write_all(&data).await {
                        error!("Write error: {}", e);
                        break;
                    }
                }
                ClientCommand::Disconnect => {
                    // Flush any pending data before closing
                    if let Err(e) = write_half.flush().await {
                        error!("Flush error: {}", e);
                    }
                    // Shutdown the write side properly
                    if let Err(e) = write_half.shutdown().await {
                        debug!("Shutdown error (expected): {}", e);
                    }
                    break;
                }
            }
        }
    })
}

// ============================================================================
// Data Handling
// ============================================================================

pub(crate) async fn handle_data(buffer: &mut Vec<u8>, callbacks: &Arc<RwLock<Callbacks>>) {
    while buffer.len() >= HEADER_SIZE {
        // Check magic
        if buffer[0..4] != MSIP_MAGIC {
            let magic = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
            let cbs = callbacks.read().await;
            for callback in &cbs.on_error {
                callback(MIPError::InvalidMagic(magic));
            }
            buffer.clear();
            return;
        }

        let payload_length = u32::from_be_bytes([
            buffer[12], buffer[13], buffer[14], buffer[15],
        ]) as usize;

        if buffer.len() < HEADER_SIZE + payload_length {
            return; // Wait for more data
        }

        // Parse header
        let header_bytes: [u8; HEADER_SIZE] = buffer[..HEADER_SIZE]
            .try_into()
            .expect("header size mismatch");

        let header = match Header::try_from(header_bytes) {
            Ok(h) => h,
            Err(e) => {
                let cbs = callbacks.read().await;
                for callback in &cbs.on_error {
                    callback(MIPError::Protocol(e.to_string()));
                }
                buffer.drain(..HEADER_SIZE);
                continue;
            }
        };

        let payload = buffer[HEADER_SIZE..HEADER_SIZE + payload_length].to_vec();
        buffer.drain(..HEADER_SIZE + payload_length);

        process_frame(header, payload, callbacks).await;
    }
}

// ============================================================================
// Frame Processing
// ============================================================================

pub(crate) async fn process_frame(
    header: Header,
    payload: Vec<u8>,
    callbacks: &Arc<RwLock<Callbacks>>,
) {
    let cbs = callbacks.read().await;

    // Emit raw frame event
    for callback in &cbs.on_frame {
        callback(header.clone(), payload.clone());
    }

    match header.frame_type {
        FrameType::Event | FrameType::Publish => {
            if let Some(msg) = parse_message(&header, &payload) {
                if header.frame_type == FrameType::Event {
                    for callback in &cbs.on_event {
                        callback(msg.clone());
                    }
                }
                for callback in &cbs.on_message {
                    callback(msg.clone());
                }
            }
        }
        FrameType::Ack => {
            for callback in &cbs.on_ack {
                callback(header.msg_id);
            }
        }
        FrameType::Pong => {
            for callback in &cbs.on_pong {
                callback();
            }
        }
        FrameType::Error => {
            let error_msg = String::from_utf8_lossy(&payload).to_string();
            for callback in &cbs.on_error {
                callback(MIPError::ServerError(error_msg.clone()));
            }
        }
        FrameType::Close => {
            debug!("Received close frame");
        }
        _ => {
            debug!("Unhandled frame type: {:?}", header.frame_type);
        }
    }
}

// ============================================================================
// Message Parsing
// ============================================================================

pub(crate) fn parse_message(header: &Header, payload: &[u8]) -> Option<MIPMessage> {
    if payload.len() < 2 {
        return None;
    }

    let topic_length = u16::from_be_bytes([payload[0], payload[1]]) as usize;

    if payload.len() < 2 + topic_length {
        return None;
    }

    let topic = String::from_utf8_lossy(&payload[2..2 + topic_length]).to_string();
    let message = String::from_utf8_lossy(&payload[2 + topic_length..]).to_string();

    Some(MIPMessage {
        header: header.clone(),
        topic,
        message,
    })
}

// ============================================================================
// Ping Task
// ============================================================================

pub(crate) fn spawn_ping_task(
    interval_ms: u64,
    connected: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    command_tx: Option<mpsc::Sender<ClientCommand>>,
    msg_id_counter: Arc<AtomicU64>,
) -> Option<JoinHandle<()>> {
    if interval_ms == 0 {
        return None;
    }

    Some(tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(Duration::from_millis(interval_ms));
        interval_timer.tick().await; // Skip first tick

        loop {
            interval_timer.tick().await;

            if !connected.load(Ordering::SeqCst) || !running.load(Ordering::SeqCst) {
                break;
            }

            if let Some(tx) = &command_tx {
                let msg_id = generate_msg_id(&msg_id_counter);
                let header = Header::new(
                    FrameType::Ping,
                    MSG_KIND_EVENT,
                    0,
                    msg_id,
                    FrameFlags::NONE,
                );
                let frame = header.encode().to_vec();
                let _ = tx.send(ClientCommand::Send(frame)).await;
            }
        }
    }))
}

// ============================================================================
// Cleanup
// ============================================================================

pub(crate) async fn cleanup_tasks(
    ping_task: &mut Option<JoinHandle<()>>,
    read_task: &mut Option<JoinHandle<()>>,
    write_task: &mut Option<JoinHandle<()>>,
) {
    // Abort ping task immediately
    if let Some(task) = ping_task.take() {
        task.abort();
    }

    // Wait for write_task to finish sending CLOSE frame (with timeout)
    if let Some(task) = write_task.take() {
        let _ = tokio::time::timeout(Duration::from_millis(500), task).await;
    }

    // Then abort read task
    if let Some(task) = read_task.take() {
        task.abort();
    }
}
