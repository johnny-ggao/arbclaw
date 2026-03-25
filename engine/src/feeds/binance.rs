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

use crate::models::{Exchange, QuoteCurrency, Symbol, Ticker, ALL_SYMBOLS};
use crate::normalizer::RateManager;

use super::TickerSender;
use super::connection::connect_ws;
use super::latency::LatencyTracker;

const WS_URL: &str = "wss://stream.binance.com:9443/ws";

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

pub async fn run(tx: TickerSender, tracker: Arc<LatencyTracker>, rate_mgr: Arc<RateManager>) -> Result<()> {
    loop {
        if let Err(e) = connect_and_stream(&tx, &tracker, &rate_mgr).await {
            error!("[Binance] connection error: {e}, reconnecting in 3s...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn connect_and_stream(tx: &TickerSender, tracker: &LatencyTracker, rate_mgr: &RateManager) -> Result<()> {
    let mut streams: Vec<String> = ALL_SYMBOLS
        .iter()
        .map(|s| format!("{}@bookTicker", symbol_to_pair(s)))
        .collect();
    streams.push("usdcusdt@bookTicker".to_string());
    let url = format!("{}/{}", WS_URL, streams.join("/"));

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
                        if let Err(e) = handle_message(&text, tx, rate_mgr) {
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

fn handle_message(text: &str, tx: &TickerSender, rate_mgr: &RateManager) -> Result<()> {
    let bt: BookTicker = serde_json::from_str(text)?;

    // USDC/USDT pair → derive USDT/USD rate (USDC ≈ 1 USD)
    if bt.s.to_uppercase() == "USDCUSDT" {
        let bid = Decimal::from_str(&bt.b)?;
        let ask = Decimal::from_str(&bt.a)?;
        let mid = (bid + ask) / Decimal::TWO;
        // mid = how many USDT per 1 USDC (≈1 USD)
        // so USDT/USD = 1/mid (how many USD per 1 USDT)
        if !mid.is_zero() {
            rate_mgr.update_usdt_usd_rate(Decimal::ONE / mid);
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
