use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;
use warp::ws::{Message, WebSocket};
use warp::Filter;

use crate::feeds::latency::LatencyTracker;
use crate::models::WsMessage;
use crate::store::{DataStore, Period};

type ClientId = u64;

pub struct WsServer {
    broadcast_tx: broadcast::Sender<String>,
}

impl WsServer {
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);
        Self { broadcast_tx }
    }

    pub fn get_sender(&self) -> broadcast::Sender<String> {
        self.broadcast_tx.clone()
    }

    pub async fn run(self, port: u16, store: Arc<DataStore>, latency: Arc<LatencyTracker>) {
        let broadcast_tx = self.broadcast_tx.clone();
        let client_counter = Arc::new(RwLock::new(0u64));

        let ws_route = warp::path("ws")
            .and(warp::ws())
            .and(warp::any().map(move || broadcast_tx.clone()))
            .and(warp::any().map(move || client_counter.clone()))
            .map(
                |ws: warp::ws::Ws,
                 broadcast_tx: broadcast::Sender<String>,
                 counter: Arc<RwLock<u64>>| {
                    ws.on_upgrade(move |socket| {
                        let id = {
                            let mut c = counter.write();
                            *c += 1;
                            *c
                        };
                        handle_client(socket, id, broadcast_tx)
                    })
                },
            );

        let store_perf = store.clone();
        let store_snap = store.clone();
        let store_mem = store.clone();
        let latency_snap = latency.clone();
        let latency_api = latency.clone();

        let performance_route = warp::path("api")
            .and(warp::path("performance"))
            .and(warp::get())
            .and(warp::query::<std::collections::HashMap<String, String>>())
            .and(warp::any().map(move || store_perf.clone()))
            .map(
                |params: std::collections::HashMap<String, String>, store: Arc<DataStore>| {
                    let period_str = params.get("period").map(|s| s.as_str()).unwrap_or("24h");
                    let period = Period::from_str(period_str);
                    let stats = store.query_performance(period);
                    warp::reply::json(&stats)
                },
            );

        let latency_route = warp::path("api")
            .and(warp::path("latency"))
            .and(warp::get())
            .and(warp::any().map(move || latency_api.clone()))
            .map(|tracker: Arc<LatencyTracker>| {
                let snapshots = tracker.snapshots();
                warp::reply::json(&snapshots)
            });

        let snapshot_route = warp::path("api")
            .and(warp::path("snapshot"))
            .and(warp::get())
            .and(warp::any().map(move || store_snap.clone()))
            .and(warp::any().map(move || latency_snap.clone()))
            .map(|store: Arc<DataStore>, tracker: Arc<LatencyTracker>| {
                let latency = tracker.snapshots();
                let snap = store.snapshot(&latency);
                warp::reply::json(&snap)
            });

        let memory_route = warp::path("api")
            .and(warp::path("memory"))
            .and(warp::get())
            .and(warp::any().map(move || store_mem.clone()))
            .map(|store: Arc<DataStore>| {
                let usage = store.memory_usage();
                warp::reply::json(&usage)
            });

        let cors = warp::cors()
            .allow_any_origin()
            .allow_methods(vec!["GET"])
            .allow_headers(vec!["Content-Type"]);

        let health = warp::path("health").map(|| warp::reply::json(&"ok"));
        let routes = ws_route
            .or(performance_route)
            .or(latency_route)
            .or(snapshot_route)
            .or(memory_route)
            .or(health)
            .with(cors);

        info!("[WsServer] starting on port {port}");
        warp::serve(routes).run(([0, 0, 0, 0], port)).await;
    }
}

async fn handle_client(ws: WebSocket, id: ClientId, broadcast_tx: broadcast::Sender<String>) {
    info!("[WsServer] client {id} connected");
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut rx = broadcast_tx.subscribe();

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if ws_tx.send(Message::text(msg)).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(_)) = ws_rx.next().await {}
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
    info!("[WsServer] client {id} disconnected");
}

pub fn broadcast_message(tx: &broadcast::Sender<String>, msg: &WsMessage) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = tx.send(json);
    }
}
