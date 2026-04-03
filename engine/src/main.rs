mod config;
mod feeds;
mod models;
mod normalizer;
mod rates;
mod store;
mod strategy;
mod ws_server;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use config::EngineConfig;
use feeds::latency::LatencyTracker;
use models::{ExchangeLatency, LatencyReport, OrderBookUpdate, Ticker, WsMessage};
use normalizer::Normalizer;
use rates::{spawn_krw_usd_poller, RateManager};
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

    let engine_config = Arc::new(EngineConfig::from_env());
    info!(
        "FX config: KRW/USD {:?} poll every {}s (stale after {}s); USDT/USD {:?} pair {}",
        engine_config.krw_usd_source,
        engine_config.krw_usd_refresh_secs,
        engine_config.krw_usd_stale_secs,
        engine_config.usdt_usd_exchange,
        engine_config.usdt_usd_pair,
    );
    let rate_manager = Arc::new(RateManager::new(&engine_config));
    let normalizer = Arc::new(Normalizer::new(rate_manager.clone()));
    let arb_engine = Arc::new(ArbitrageEngine::new());
    let data_store = Arc::new(DataStore::new());
    // Try multiple paths for seed data (Docker: /data, local: ../data or data)
    for path in &["/data/opportunities.json", "data/opportunities.json", "../data/opportunities.json"] {
        if std::path::Path::new(path).exists() {
            data_store.load_seed_signals(path);
            break;
        }
    }
    let latency_tracker = Arc::new(LatencyTracker::new());

    let ws_server = WsServer::new();
    let ws_broadcast = ws_server.get_sender();

    let store_clone = data_store.clone();
    let latency_clone = latency_tracker.clone();
    tokio::spawn(async move {
        ws_server.run(WS_PORT, store_clone, latency_clone).await;
    });

    spawn_krw_usd_poller(rate_manager.clone(), engine_config.clone());

    // Spawn feeds
    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    let rm = rate_manager.clone();
    let cfg = engine_config.clone();
    tokio::spawn(async move { feeds::binance::run(tx, ob, lt, rm, cfg).await });

    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    let rm = rate_manager.clone();
    let cfg = engine_config.clone();
    tokio::spawn(async move { feeds::bybit::run(tx, ob, lt, rm, cfg).await });

    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    tokio::spawn(async move { feeds::upbit::run(tx, ob, lt).await });

    let tx = ticker_tx.clone();
    let ob = ob_tx.clone();
    let lt = latency_tracker.clone();
    tokio::spawn(async move { feeds::bithumb::run(tx, ob, lt).await });

    // Batched WS push buffers (flushed at 1Hz)
    type TickerKey = (models::Exchange, models::Symbol);
    let mut pending_tickers: HashMap<TickerKey, models::NormalizedTicker> = HashMap::new();
    let mut pending_signals: Vec<models::ArbitrageSignal> = Vec::new();
    let mut pending_rate: Option<models::ExchangeRate> = None;
    let mut pending_latency: Option<LatencyReport> = None;
    let mut pending_orderbooks: HashMap<String, OrderBookUpdate> = HashMap::new();

    let mut ob_rx = ob_tx.subscribe();
    let mut ticker_rx = ticker_tx.subscribe();
    let mut signal_count: u64 = 0;
    let mut tick_count: u64 = 0;
    let mut flush_interval = interval(Duration::from_secs(1));

    info!("Processing loop started (1Hz WS push). WS server on ws://0.0.0.0:{WS_PORT}/ws");
    info!("REST APIs: /api/performance, /api/latency, /api/snapshot, /api/memory");

    loop {
        tokio::select! {
            // --- Orderbook updates: buffer latest per key ---
            result = ob_rx.recv() => {
                match result {
                    Ok(ob) => {
                        let key = format!("{}:{}:{:?}", ob.exchange, ob.symbol, ob.quote_currency);
                        pending_orderbooks.insert(key, ob);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("orderbook receiver lagged by {n}");
                    }
                    Err(_) => break,
                }
            }

            // --- Ticker updates: process + buffer ---
            result = ticker_rx.recv() => {
                match result {
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

                        // Store immediately (full resolution)
                        data_store.push_ticker(&normalized);

                        // Buffer for WS push (keep latest per exchange+symbol)
                        let key = (normalized.exchange, normalized.symbol);
                        pending_tickers.insert(key, normalized.clone());

                        if ticker.symbol == models::Symbol::BTC && tick_count % 100 == 0 {
                            if let Some(rate) = rate_manager.get_rate() {
                                data_store.push_rate(&rate);
                                pending_rate = Some(rate);
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
                            pending_latency = Some(report);
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
                            pending_signals.push(signal);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("processing loop lagged by {n} messages");
                    }
                    Err(_) => break,
                }
            }

            // --- 1Hz flush: push all buffered data to WS clients ---
            _ = flush_interval.tick() => {
                for (_, ticker) in pending_tickers.drain() {
                    broadcast_message(&ws_broadcast, &WsMessage::Ticker(ticker));
                }
                for signal in pending_signals.drain(..) {
                    broadcast_message(&ws_broadcast, &WsMessage::Signal(signal));
                }
                if let Some(rate) = pending_rate.take() {
                    broadcast_message(&ws_broadcast, &WsMessage::Rate(rate));
                }
                if let Some(latency) = pending_latency.take() {
                    broadcast_message(&ws_broadcast, &WsMessage::Latency(latency));
                }
                for (_, ob) in pending_orderbooks.drain() {
                    broadcast_message(&ws_broadcast, &WsMessage::OrderBook(ob));
                }
            }
        }
    }
}
