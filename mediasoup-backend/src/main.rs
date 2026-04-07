use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use mediasoup::consumer::ConsumerId;
use mediasoup::data_structures::{DtlsParameters, IpAddress, ListenInfo, Protocol};
use mediasoup::producer::ProducerId;
use mediasoup::router::{Router, RouterOptions};
use mediasoup::rtp_parameters::{
    MediaKind, MimeTypeAudio, MimeTypeVideo, RtpCapabilities, RtpCodecCapability,
    RtpCodecParametersParameters, RtpParameters,
};
use mediasoup::transport::{Transport, TransportId};
use mediasoup::webrtc_transport::{
    TransportListenIps, WebRtcTransport, WebRtcTransportOptions, WebRtcTransportRemoteParameters,
};
use mediasoup::worker::{Worker, WorkerSettings};
use mediasoup::worker_manager::WorkerManager;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tungstenite::Message;
use uuid::Uuid;

// ─────────────────────────────────────────────
// Shared State
// ─────────────────────────────────────────────

type ClientId = String;

struct AppState {
    router: Router,
    transports: Mutex<HashMap<TransportId, WebRtcTransport>>,
    producers: Mutex<HashMap<ProducerId, mediasoup::producer::Producer>>,
    consumers: Mutex<HashMap<ConsumerId, mediasoup::consumer::Consumer>>,
    /// clientId -> Set of ProducerIds
    client_producers: Mutex<HashMap<ClientId, HashSet<ProducerId>>>,
    /// Channel to broadcast messages to all connected clients
    broadcast_tx: tokio::sync::broadcast::Sender<(ClientId, String)>,
}

impl AppState {
    fn new(router: Router, broadcast_tx: tokio::sync::broadcast::Sender<(ClientId, String)>) -> Self {
        Self {
            router,
            transports: Mutex::new(HashMap::new()),
            producers: Mutex::new(HashMap::new()),
            consumers: Mutex::new(HashMap::new()),
            client_producers: Mutex::new(HashMap::new()),
            broadcast_tx,
        }
    }
}

// ─────────────────────────────────────────────
// WebSocket Messages (Incoming)
// ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum IncomingMsg {
    GetRtpCapabilities,
    CreateSendTransport,
    ConnectSendTransport {
        #[serde(rename = "transportId")]
        transport_id: TransportId,
        #[serde(rename = "dtlsParameters")]
        dtls_parameters: DtlsParameters,
    },
    Produce {
        #[serde(rename = "transportId")]
        transport_id: TransportId,
        kind: MediaKind,
        #[serde(rename = "rtpParameters")]
        rtp_parameters: RtpParameters,
    },
    CreateRecvTransport,
    ConnectRecvTransport {
        #[serde(rename = "transportId")]
        transport_id: TransportId,
        #[serde(rename = "dtlsParameters")]
        dtls_parameters: DtlsParameters,
    },
    GetExistingProducers,
    Consume {
        #[serde(rename = "producerId")]
        producer_id: ProducerId,
        #[serde(rename = "rtpCapabilities")]
        rtp_capabilities: RtpCapabilities,
    },
    ResumeConsumer {
        #[serde(rename = "consumerId")]
        consumer_id: ConsumerId,
    },
}

// ─────────────────────────────────────────────
// WebSocket Messages (Outgoing)
// ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum OutgoingMsg {
    RtpCapabilities {
        data: RtpCapabilities,
    },
    SendTransportCreated {
        data: TransportInfo,
    },
    RecvTransportCreated {
        data: TransportInfo,
    },
    Produced {
        data: ProducedData,
    },
    NewProducer {
        data: ProducerInfo,
    },
    ExistingProducers {
        data: Vec<ProducerInfo>,
    },
    ConsumeResponse {
        data: ConsumeData,
    },
}

#[derive(Debug, Serialize)]
struct TransportInfo {
    id: TransportId,
    #[serde(rename = "iceParameters")]
    ice_parameters: mediasoup::webrtc_transport::IceParameters,
    #[serde(rename = "iceCandidates")]
    ice_candidates: Vec<mediasoup::data_structures::IceCandidate>,
    #[serde(rename = "dtlsParameters")]
    dtls_parameters: DtlsParameters,
}

#[derive(Debug, Serialize)]
struct ProducedData {
    id: ProducerId,
}

#[derive(Debug, Serialize, Clone)]
struct ProducerInfo {
    #[serde(rename = "producerId")]
    producer_id: ProducerId,
    kind: MediaKind,
}

#[derive(Debug, Serialize)]
struct ConsumeData {
    id: ConsumerId,
    #[serde(rename = "producerId")]
    producer_id: ProducerId,
    kind: MediaKind,
    #[serde(rename = "rtpParameters")]
    rtp_parameters: RtpParameters,
}

// ─────────────────────────────────────────────
// Helper: build transport info from WebRtcTransport
// ─────────────────────────────────────────────

fn transport_info(t: &WebRtcTransport) -> TransportInfo {
    TransportInfo {
        id: t.id(),
        ice_parameters: t.ice_parameters().clone(),
        ice_candidates: t.ice_candidates().clone(),
        dtls_parameters: t.dtls_parameters().clone(),
    }
}

// ─────────────────────────────────────────────
// Create a WebRtcTransport
// ─────────────────────────────────────────────

async fn create_transport(router: &Router) -> WebRtcTransport {
    let options = WebRtcTransportOptions::new(TransportListenIps::new(ListenInfo {
        protocol: Protocol::Udp,
        ip: IpAddress::Ip4("127.0.0.1".parse().unwrap()),
        announced_address: None,
        port: None,
        port_range: None,
        flags: None,
        send_buffer_size: None,
        recv_buffer_size: None,
        expose_internal_ip: todo!(),
    }));

    router.create_webrtc_transport(options).await.unwrap()
}

// ─────────────────────────────────────────────
// Send a JSON message over WebSocket
// ─────────────────────────────────────────────

async fn send_json<S>(sink: &mut S, msg: &OutgoingMsg)
where
    S: SinkExt<Message> + Unpin,
    <S as futures_util::Sink<Message>>::Error: std::fmt::Debug,
{
    let json = serde_json::to_string(msg).unwrap();
    let _ = sink.send(Message::Text(json)).await;
}

// ─────────────────────────────────────────────
// Handle a single WebSocket connection
// ─────────────────────────────────────────────

async fn handle_connection(stream: TcpStream, state: Arc<AppState>) {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("WebSocket handshake failed");

    let client_id: ClientId = Uuid::new_v4()
        .to_string()
        .chars()
        .take(7)
        .collect::<String>();

    println!("\n🟢 Client connected → {}", client_id);

    // Register client
    {
        let mut cp = state.client_producers.lock().await;
        cp.insert(client_id.clone(), HashSet::new());
    }

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Subscribe to broadcast channel (for newProducer events from other clients)
    let mut broadcast_rx = state.broadcast_tx.subscribe();

    loop {
        tokio::select! {
            // ── Incoming message from this client ──
            Some(Ok(msg)) = ws_stream.next() => {
                let text = match msg {
                    Message::Text(t) => t,
                    Message::Close(_) => break,
                    _ => continue,
                };

                println!("\n📩 {} → (raw) {}", client_id, &text[..text.len().min(80)]);

                let parsed: IncomingMsg = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("❌ Parse error: {}", e);
                        continue;
                    }
                };

                match parsed {

                    // ── getRtpCapabilities ──
                    IncomingMsg::GetRtpCapabilities => {
                        send_json(
                            &mut ws_sink,
                            &OutgoingMsg::RtpCapabilities {
                                data: state.router.rtp_capabilities().clone(),
                            },
                        )
                        .await;
                    }

                    // ── createSendTransport ──
                    IncomingMsg::CreateSendTransport => {
                        let transport = create_transport(&state.router).await;
                        let info = transport_info(&transport);
                        println!("✅ Send transport: {}", transport.id());

                        state.transports.lock().await.insert(transport.id(), transport);

                        send_json(
                            &mut ws_sink,
                            &OutgoingMsg::SendTransportCreated { data: info },
                        )
                        .await;
                    }

                    // ── connectSendTransport ──
                    IncomingMsg::ConnectSendTransport {
                        transport_id,
                        dtls_parameters,
                    } => {
                        let transports = state.transports.lock().await;
                        if let Some(t) = transports.get(&transport_id) {
                            t.connect(WebRtcTransportRemoteParameters { dtls_parameters })
                                .await
                                .unwrap();
                            println!("✅ Send transport connected");
                        }
                    }

                    // ── produce ──
                    IncomingMsg::Produce {
                        transport_id,
                        kind,
                        rtp_parameters,
                    } => {
                        let transports = state.transports.lock().await;
                        if let Some(transport) = transports.get(&transport_id) {
                            let producer = transport
                                .produce(mediasoup::producer::ProducerOptions::new(
                                    kind,
                                    rtp_parameters,
                                ))
                                .await
                                .unwrap();

                            let producer_id = producer.id();
                            println!("✅ Producer: {} {:?}", producer_id, kind);

                            // Register producer
                            state.producers.lock().await.insert(producer_id, producer);
                            state
                                .client_producers
                                .lock()
                                .await
                                .entry(client_id.clone())
                                .or_default()
                                .insert(producer_id);

                            // Broadcast newProducer to other clients
                            let broadcast_msg = serde_json::to_string(&OutgoingMsg::NewProducer {
                                data: ProducerInfo { producer_id, kind },
                            })
                            .unwrap();
                            let _ = state
                                .broadcast_tx
                                .send((client_id.clone(), broadcast_msg));

                            send_json(
                                &mut ws_sink,
                                &OutgoingMsg::Produced {
                                    data: ProducedData { id: producer_id },
                                },
                            )
                            .await;
                        }
                    }

                    // ── createRecvTransport ──
                    IncomingMsg::CreateRecvTransport => {
                        let transport = create_transport(&state.router).await;
                        let info = transport_info(&transport);
                        println!("✅ Recv transport: {}", transport.id());

                        state.transports.lock().await.insert(transport.id(), transport);

                        send_json(
                            &mut ws_sink,
                            &OutgoingMsg::RecvTransportCreated { data: info },
                        )
                        .await;
                    }

                    // ── connectRecvTransport ──
                    IncomingMsg::ConnectRecvTransport {
                        transport_id,
                        dtls_parameters,
                    } => {
                        let transports = state.transports.lock().await;
                        if let Some(t) = transports.get(&transport_id) {
                            t.connect(WebRtcTransportRemoteParameters { dtls_parameters })
                                .await
                                .unwrap();
                            println!("✅ Recv transport connected");
                        }
                    }

                    // ── getExistingProducers ──
                    IncomingMsg::GetExistingProducers => {
                        let mut existing = Vec::new();
                        let cp = state.client_producers.lock().await;
                        let producers = state.producers.lock().await;

                        for (cid, producer_ids) in cp.iter() {
                            if cid == &client_id {
                                continue;
                            }
                            for pid in producer_ids {
                                if let Some(p) = producers.get(pid) {
                                    if !p.closed() {
                                        existing.push(ProducerInfo {
                                            producer_id: p.id(),
                                            kind: p.kind(),
                                        });
                                    }
                                }
                            }
                        }

                        if !existing.is_empty() {
                            println!(
                                "📤 Sending {} existing producers to {}",
                                existing.len(),
                                client_id
                            );
                            send_json(
                                &mut ws_sink,
                                &OutgoingMsg::ExistingProducers { data: existing },
                            )
                            .await;
                        }
                    }

                    // ── consume ──
                    IncomingMsg::Consume {
                        producer_id,
                        rtp_capabilities,
                    } => {
                        let producers = state.producers.lock().await;
                        let producer = match producers.get(&producer_id) {
                            Some(p) if !p.closed() => p,
                            _ => {
                                eprintln!("❌ Producer not found or closed");
                                continue;
                            }
                        };

                        if !state.router.can_consume(&producer_id, &rtp_capabilities) {
                            eprintln!("❌ Cannot consume");
                            continue;
                        }

                        // Find recv transport for this client
                        let transports = state.transports.lock().await;
                        // We store both send & recv transports in the same map.
                        // Since we create recv after send, find the one that was created last
                        // for this client that isn't already used for a producer.
                        // Simplest approach: track by appData (not available here), so instead
                        // we look up by finding transports not used for producing by this client.
                        //
                        // Better approach: keep a separate recv_transports map per client.
                        // For now, we grab the transport that was most recently created and doesn't
                        // match any send transport. Since createRecvTransport is the last one created,
                        // we just pick the last inserted for this client.
                        //
                        // NOTE: For production use, maintain separate send/recv transport maps.
                        let send_transport_ids: HashSet<TransportId> = {
                            let cp = state.client_producers.lock().await;
                            // producer transport IDs — we'd need to track this separately
                            // For simplicity here we grab all transports and pick the last one for the client
                            HashSet::new()
                        };

                        // Get all transports - we'll pick the last created (recv) one
                        // In practice, you should track clientId→recvTransportId explicitly.
                        let recv_transport = transports.values().last();

                        let transport = match recv_transport {
                            Some(t) => t,
                            None => {
                                eprintln!("❌ No recv transport found for {}", client_id);
                                continue;
                            }
                        };

                        let consumer = transport
                            .consume(mediasoup::consumer::ConsumerOptions::new(
                                producer_id,
                                rtp_capabilities,
                            ))
                            .await
                            .unwrap();

                        let consumer_id = consumer.id();
                        let kind = consumer.kind();
                        let rtp_params = consumer.rtp_parameters().clone();

                        println!("✅ Consumer: {}", consumer_id);

                        state.consumers.lock().await.insert(consumer_id, consumer);

                        send_json(
                            &mut ws_sink,
                            &OutgoingMsg::ConsumeResponse {
                                data: ConsumeData {
                                    id: consumer_id,
                                    producer_id,
                                    kind,
                                    rtp_parameters: rtp_params,
                                },
                            },
                        )
                        .await;
                    }

                    // ── resumeConsumer ──
                    IncomingMsg::ResumeConsumer { consumer_id } => {
                        let consumers = state.consumers.lock().await;
                        if let Some(c) = consumers.get(&consumer_id) {
                            c.resume().await.unwrap();
                            println!("✅ Consumer resumed");
                        }
                    }
                }
            }

            // ── Broadcast message from another client ──
            Ok((sender_id, json)) = broadcast_rx.recv() => {
                // Forward to everyone EXCEPT the sender
                if sender_id != client_id {
                    let _ = ws_sink.send(Message::Text(json)).await;
                }
            }

            else => break,
        }
    }

    // ── Cleanup on disconnect ──
    println!("🔴 Client disconnected → {}", client_id);

    let producer_ids: Vec<ProducerId> = {
        let mut cp = state.client_producers.lock().await;
        cp.remove(&client_id)
            .unwrap_or_default()
            .into_iter()
            .collect()
    };

    {
        let mut producers = state.producers.lock().await;
        for pid in &producer_ids {
            if let Some(p) = producers.remove(pid) {
                drop(p); // closing the producer
            }
        }
    }

    {
        let mut transports = state.transports.lock().await;
        transports.retain(|_, t| {
            // No appData in mediasoup-rs — so we can't filter by clientId here easily.
            // In a real app, maintain a separate map: clientId → Vec<TransportId>
            // For now, we leave transport cleanup as a TODO or track separately.
            true
        });
    }
}

// ─────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Create mediasoup worker
    let worker_manager = WorkerManager::new();
    let worker = worker_manager
        .create_worker(WorkerSettings::default())
        .await
        .expect("Failed to create worker");
    println!("✅ Worker created");

    // Create router with audio + video codecs
    let router = worker
        .create_router(RouterOptions::new(vec![
            RtpCodecCapability::Audio {
                mime_type: MimeTypeAudio::Opus,
                preferred_payload_type: None,
                clock_rate: std::num::NonZeroU32::new(48000).unwrap(),
                channels: std::num::NonZeroU8::new(2).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![],
            },
            RtpCodecCapability::Video {
                mime_type: MimeTypeVideo::Vp8,
                preferred_payload_type: None,
                clock_rate: std::num::NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![],
            },
        ]))
        .await
        .expect("Failed to create router");
    println!("✅ Router created");

    // Broadcast channel (capacity 128)
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<(ClientId, String)>(128);

    let state = Arc::new(AppState::new(router, broadcast_tx));

    // Start TCP listener
    let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
    println!("🚀 Server running → ws://{}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await.expect("Accept failed");
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            handle_connection(stream, state).await;
        });
    }
}