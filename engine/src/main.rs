mod feeds;
mod models;
mod normalizer;
mod store;
mod strategy;
mod ws_server;

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use feeds::latency::LatencyTracker;
use models::{ExchangeLatency, LatencyReport, OrderBookUpdate, Ticker, WsMessage};
use normalizer::{Normalizer, RateManager, spawn_frankfurter_poller};
use store::DataStore;
use strategy::ArbitrageEngine;
use ws_server::{WsServer, broadcast_message};

const WS_PORT: u16 = 8765;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("CEX Arbitrage Engine starting...");

    let (ticker_tx, _) = broadcast::channel::<Ticker>(4096);
    let (ob_tx, _) = broadcast::channel::<OrderBookUpdate>(2048);

    let rate_manager = Arc::new(RateManager::new());
    let normalizer = Arc::new(Normalizer::new(rate_manager.clone()));
    let arb_engine = Arc::new(ArbitrageEngine::new());
    let data_store = Arc::new(DataStore::new());
    let latency_tracker = Arc::new(LatencyTracker::new());

    let ws_server = WsServer::new();
    let ws_broadcast = ws_server.get_sender();

    let store_clone = data_store.clone();
    let latency_clone = latency_tracker.clone();
    tokio::spawn(async move {
        ws_server.run(WS_PORT, store_clone, latency_clone).await;
    });

    // Spawn Frankfurter KRW/USD rate poller (ECB data, refreshes every 600s)
    spawn_frankfurter_poller(rate_manager.clone(), 600);

    // Spawn feeds
    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    let rm = rate_manager.clone();
    tokio::spawn(async move { feeds::binance::run(tx, ob, lt, rm).await });

    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    tokio::spawn(async move { feeds::bybit::run(tx, ob, lt).await });

    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    tokio::spawn(async move { feeds::upbit::run(tx, ob, lt).await });

    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    tokio::spawn(async move { feeds::bithumb::run(tx, ob, lt).await });

    // Spawn orderbook forwarder (broadcasts to WS clients)
    let ws_bc_ob = ws_broadcast.clone();
    let mut ob_rx = ob_tx.subscribe();
    tokio::spawn(async move {
        loop {
            match ob_rx.recv().await {
                Ok(ob) => {
                    broadcast_message(&ws_bc_ob, &WsMessage::OrderBook(ob));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("orderbook forwarder lagged by {n}");
                }
                Err(_) => break,
            }
        }
    });

    let mut ticker_rx = ticker_tx.subscribe();
    let mut signal_count: u64 = 0;
    let mut tick_count: u64 = 0;

    info!("Processing loop started. WS server on ws://0.0.0.0:{WS_PORT}/ws");
    info!("REST APIs: /api/performance, /api/latency, /api/snapshot, /api/memory");

    loop {
        match ticker_rx.recv().await {
            Ok(ticker) => {
                tick_count += 1;
                if tick_count % 5000 == 0 {
                    info!("processed {tick_count} ticks, {signal_count} signals generated");
                    let mem = data_store.memory_usage();
                    info!(
                        "MEMORY: signals={}/{} tickers={} entries ({} pairs) rates={}/{} ~{:.1}MB",
                        mem.signals_count, mem.signals_cap,
                        mem.ticker_total_entries, mem.ticker_pairs,
                        mem.rates_count, mem.rates_cap,
                        mem.estimated_mb,
                    );
                }

                let normalized = match normalizer.process(&ticker) {
                    Some(n) => n,
                    None => {
                        if tick_count < 10 {
                            warn!(
                                "skipping {} {} (no exchange rate yet)",
                                ticker.exchange, ticker.symbol
                            );
                        }
                        continue;
                    }
                };

                if tick_count % 3000 == 0 {
                    let rate_info = rate_manager
                        .get_rate()
                        .map(|r| format!(
                            "KRW/USDT={:.2} USDT/USD={:.6} KRW/USD={:.2}",
                            r.krw_per_usdt, r.usdt_per_usd, r.krw_per_usd
                        ))
                        .unwrap_or_else(|| "NO RATE".to_string());
                    info!(
                        "SNAPSHOT: {} {} bid_usd={:.2} ask_usd={:.2} | {rate_info}",
                        normalized.exchange,
                        normalized.symbol,
                        normalized.best_bid_usd,
                        normalized.best_ask_usd,
                    );
                }

                data_store.push_ticker(&normalized);
                broadcast_message(&ws_broadcast, &WsMessage::Ticker(normalized.clone()));

                if ticker.symbol == models::Symbol::BTC && tick_count % 100 == 0 {
                    if let Some(rate) = rate_manager.get_rate() {
                        data_store.push_rate(&rate);
                        broadcast_message(&ws_broadcast, &WsMessage::Rate(rate));
                    }
                }

                if tick_count % 500 == 0 {
                    let snapshots = latency_tracker.snapshots();
                    let report = LatencyReport {
                        exchanges: snapshots
                            .into_iter()
                            .map(|s| ExchangeLatency {
                                exchange: s.exchange,
                                last_rtt_ms: s.last_rtt_ms,
                                avg_rtt_ms: s.avg_rtt_ms,
                                min_rtt_ms: s.min_rtt_ms,
                                max_rtt_ms: s.max_rtt_ms,
                                samples: s.samples,
                            })
                            .collect(),
                    };
                    broadcast_message(&ws_broadcast, &WsMessage::Latency(report));
                }

                let signals = arb_engine.update(normalized);
                for signal in signals {
                    signal_count += 1;
                    data_store.push_signal(&signal);
                    if signal_count % 500 == 1 {
                        info!(
                            "SIGNAL #{signal_count}: {} {}: {} → {} net={:.3}% profit=${:.2}",
                            signal.symbol,
                            signal.buy_exchange,
                            signal.buy_exchange,
                            signal.sell_exchange,
                            signal.net_spread_pct,
                            signal.estimated_profit_usd,
                        );
                    }
                    broadcast_message(&ws_broadcast, &WsMessage::Signal(signal));
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("processing loop lagged by {n} messages");
            }
            Err(_) => break,
        }
    }
}
