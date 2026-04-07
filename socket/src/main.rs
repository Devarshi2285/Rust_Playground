use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info};

// ── Message types ─────────────────────────────────────────────────────────────

/// Client sends this as the very first message after connecting
/// { "event": "register", "id": "alice" }
#[derive(Debug, Deserialize)]
struct RegisterMessage {
    event: String,
    id: String,
}

/// Every signal message carries the sender's id
/// { "event": "offer"|"answer"|"ice-candidate", "id": "alice", "data": "..." }
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalMessage {
    event: String,
    id: String,
    data: String,
}

type Tx = broadcast::Sender<String>;
type PeerMap = Arc<Mutex<HashMap<String, Tx>>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let addr = "0.0.0.0:9001";
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");
    info!("Signaling server on ws://{}", addr);

    let peer_map: PeerMap = Arc::new(Mutex::new(HashMap::new()));

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(stream, addr, peer_map.clone()));
    }
}

async fn handle_connection(stream: TcpStream, addr: SocketAddr, peer_map: PeerMap) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => { error!("Handshake failed {}: {}", addr, e); return; }
    };

    let (mut sink, mut source) = ws_stream.split();

    // ── 1. Wait for register event ────────────────────────────────────────────
    let client_id = loop {
        match source.next().await {
            Some(Ok(Message::Text(text))) => {
                if let Ok(reg) = serde_json::from_str::<RegisterMessage>(&text) {
                    if reg.event == "register" && !reg.id.is_empty() {
                        info!("[{}] registered id='{}'", addr, reg.id);
                        break reg.id;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            _ => {}
        }
    };

    // ── 2. Add to peer map ────────────────────────────────────────────────────
    let (tx, _) = broadcast::channel::<String>(64);
    peer_map.lock().await.insert(client_id.clone(), tx.clone());

    // ── 3. Sender task: broadcast rx → this peer's WS ────────────────────────
    let mut rx = tx.subscribe();
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sink.send(Message::Text(msg.into())).await.is_err() { break; }
        }
    });

    // ── 4. Receive task: WS → broadcast to ALL peers (including sender) ───────
    let pm = peer_map.clone();
    let cid = client_id.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = source.next().await {
            if let Ok(mut sig) = serde_json::from_str::<SignalMessage>(&text) {
                if !matches!(sig.event.as_str(), "offer" | "answer" | "ice-candidate") {
                    continue;
                }
                // Stamp sender id (client can send anything; server enforces it)
                sig.id = cid.clone();
                let outgoing = serde_json::to_string(&sig).unwrap();
                info!("[{}] event='{}' → echo to all", cid, sig.event);

                let peers = pm.lock().await;
                for peer_tx in peers.values() {
                    let _ = peer_tx.send(outgoing.clone());
                }
            }
        }
    });

    tokio::select! { _ = recv_task => {} _ = send_task => {} }

    peer_map.lock().await.remove(&client_id);
    info!("'{}' disconnected", client_id);
}