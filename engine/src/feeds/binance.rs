use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::config::{EngineConfig, UsdtUsdExchange};
use crate::models::{Exchange, OrderBookUpdate, PriceLevel, QuoteCurrency, Symbol, Ticker, ALL_SYMBOLS};
use crate::rates::RateManager;

use super::connection::connect_ws;
use super::latency::LatencyTracker;
use super::{OrderBookSender, TickerSender};

const WS_URL: &str = "wss://stream.binance.com:9443/stream";

#[derive(Debug, Deserialize)]
struct BookTicker {
    s: String,
    b: String,
    #[serde(rename = "B")]
    bq: String,
    a: String,
    #[serde(rename = "A")]
    aq: String,
}

#[derive(Debug, Deserialize)]
struct DepthSnapshot {
    #[serde(rename = "lastUpdateId")]
    _last_update_id: Option<u64>,
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

fn symbol_to_pair(s: &Symbol) -> &'static str {
    match s {
        Symbol::BTC => "btcusdt",
        Symbol::ETH => "ethusdt",
        Symbol::SOL => "solusdt",
        Symbol::XRP => "xrpusdt",
    }
}

fn pair_to_symbol(pair: &str) -> Option<Symbol> {
    match pair.to_uppercase().as_str() {
        "BTCUSDT" => Some(Symbol::BTC),
        "ETHUSDT" => Some(Symbol::ETH),
        "SOLUSDT" => Some(Symbol::SOL),
        "XRPUSDT" => Some(Symbol::XRP),
        _ => None,
    }
}

fn stream_to_symbol(stream: &str) -> Option<Symbol> {
    // "btcusdt@depth5@100ms" → "BTCUSDT"
    let pair = stream.split('@').next()?;
    pair_to_symbol(pair)
}

pub async fn run(
    tx: TickerSender,
    ob_tx: OrderBookSender,
    tracker: Arc<LatencyTracker>,
    rate_mgr: Arc<RateManager>,
    config: Arc<EngineConfig>,
) -> Result<()> {
    loop {
        if let Err(e) = connect_and_stream(&tx, &ob_tx, &tracker, &rate_mgr, config.as_ref()).await {
            error!("[Binance] connection error: {e}, reconnecting in 3s...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn connect_and_stream(
    tx: &TickerSender,
    ob_tx: &OrderBookSender,
    tracker: &LatencyTracker,
    rate_mgr: &RateManager,
    config: &EngineConfig,
) -> Result<()> {
    let mut streams: Vec<String> = ALL_SYMBOLS
        .iter()
        .flat_map(|s| {
            let p = symbol_to_pair(s);
            vec![
                format!("{p}@bookTicker"),
                format!("{p}@depth5@100ms"),
            ]
        })
        .collect();
    if config.usdt_usd_exchange == UsdtUsdExchange::Binance {
        streams.push(format!("{}@bookTicker", config.usdt_pair_lower()));
    }
    let url = format!("{}?streams={}", WS_URL, streams.join("/"));

    info!("[Binance] connecting to {url}");
    let ws_stream = connect_ws(&url).await?;
    info!("[Binance] connected");
    let (mut write, mut read) = ws_stream.split();

    let ping_interval = tokio::time::interval(std::time::Duration::from_secs(15));
    tokio::pin!(ping_interval);
    let mut ping_sent_at: Option<Instant> = None;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_message(&text, tx, ob_tx, rate_mgr, config) {
                            warn!("[Binance] parse error: {e}");
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        if let Some(sent) = ping_sent_at.take() {
                            let rtt = sent.elapsed().as_secs_f64() * 1000.0;
                            tracker.record(Exchange::Binance, rtt);
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        warn!("[Binance] server closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("[Binance] ws error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                ping_sent_at = Some(Instant::now());
                if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                    error!("[Binance] ping failed: {e}");
                    break;
                }
            }
        }
    }
    Ok(())
}

// Combined stream messages come wrapped: {"stream":"...","data":{...}}
#[derive(Debug, Deserialize)]
struct CombinedMsg {
    stream: Option<String>,
    data: Option<serde_json::Value>,
    // bookTicker fields (when not combined)
    s: Option<String>,
}

fn handle_message(
    text: &str,
    tx: &TickerSender,
    ob_tx: &OrderBookSender,
    rate_mgr: &RateManager,
    config: &EngineConfig,
) -> Result<()> {
    // Combined stream format: check if it has "stream" field
    let v: serde_json::Value = serde_json::from_str(text)?;

    if v.get("stream").is_some() {
        let stream = v["stream"].as_str().unwrap_or("");
        let data = &v["data"];

        if stream.contains("@depth5") {
            // Depth snapshot
            let snap: DepthSnapshot = serde_json::from_value(data.clone())?;
            if let Some(symbol) = stream_to_symbol(stream) {
                let bids: Vec<PriceLevel> = snap.bids.iter().take(5).filter_map(|l| {
                    Some(PriceLevel { price: Decimal::from_str(&l[0]).ok()?, qty: Decimal::from_str(&l[1]).ok()? })
                }).collect();
                let asks: Vec<PriceLevel> = snap.asks.iter().take(5).filter_map(|l| {
                    Some(PriceLevel { price: Decimal::from_str(&l[0]).ok()?, qty: Decimal::from_str(&l[1]).ok()? })
                }).collect();
                let _ = ob_tx.send(OrderBookUpdate {
                    exchange: Exchange::Binance,
                    symbol,
                    bids,
                    asks,
                    quote_currency: QuoteCurrency::USDT,
                    timestamp: Utc::now(),
                });
            }
            return Ok(());
        }

        if stream.contains("@bookTicker") {
            let bt: BookTicker = serde_json::from_value(data.clone())?;
            return handle_book_ticker(&bt, tx, rate_mgr, config);
        }

        return Ok(());
    }

    // Non-combined format (shouldn't happen with combined streams, but fallback)
    let bt: BookTicker = serde_json::from_str(text)?;
    handle_book_ticker(&bt, tx, rate_mgr, config)
}

fn handle_book_ticker(
    bt: &BookTicker,
    tx: &TickerSender,
    rate_mgr: &RateManager,
    config: &EngineConfig,
) -> Result<()> {
    if config.usdt_usd_exchange == UsdtUsdExchange::Binance && bt.s.to_uppercase() == config.usdt_usd_pair {
        let bid = Decimal::from_str(&bt.b)?;
        let ask = Decimal::from_str(&bt.a)?;
        let mid = (bid + ask) / Decimal::TWO;
        if !mid.is_zero() {
            rate_mgr.update_usdt_usd_from_pair_mid(mid);
        }
        return Ok(());
    }

    let symbol = pair_to_symbol(&bt.s).ok_or_else(|| anyhow::anyhow!("unknown pair: {}", bt.s))?;
    let now = Utc::now();
    let ticker = Ticker {
        exchange: Exchange::Binance,
        symbol,
        best_bid: Decimal::from_str(&bt.b)?,
        best_bid_qty: Decimal::from_str(&bt.bq)?,
        best_ask: Decimal::from_str(&bt.a)?,
        best_ask_qty: Decimal::from_str(&bt.aq)?,
        quote_currency: QuoteCurrency::USDT,
        timestamp: now,
        local_timestamp: now,
    };
    let _ = tx.send(ticker);
    Ok(())
}
