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

use crate::models::{Exchange, OrderBookUpdate, PriceLevel, QuoteCurrency, Symbol, Ticker, ALL_SYMBOLS};

use super::connection::connect_ws;
use super::latency::LatencyTracker;
use super::{OrderBookSender, TickerSender};

const WS_URL: &str = "wss://stream.bybit.com/v5/public/spot";

#[derive(Debug, Deserialize)]
struct WsResponse {
    topic: Option<String>,
    data: Option<OrderbookData>,
    #[allow(dead_code)]
    op: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrderbookData {
    s: String,
    b: Vec<[String; 2]>,
    a: Vec<[String; 2]>,
}

fn symbol_to_pair(s: &Symbol) -> &'static str {
    match s {
        Symbol::BTC => "BTCUSDT",
        Symbol::ETH => "ETHUSDT",
        Symbol::SOL => "SOLUSDT",
        Symbol::XRP => "XRPUSDT",
    }
}

fn pair_to_symbol(pair: &str) -> Option<Symbol> {
    match pair {
        "BTCUSDT" => Some(Symbol::BTC),
        "ETHUSDT" => Some(Symbol::ETH),
        "SOLUSDT" => Some(Symbol::SOL),
        "XRPUSDT" => Some(Symbol::XRP),
        _ => None,
    }
}

pub async fn run(tx: TickerSender, ob_tx: OrderBookSender, tracker: Arc<LatencyTracker>) -> Result<()> {
    loop {
        if let Err(e) = connect_and_stream(&tx, &ob_tx, &tracker).await {
            error!("[Bybit] connection error: {e}, reconnecting in 3s...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn connect_and_stream(tx: &TickerSender, ob_tx: &OrderBookSender, tracker: &LatencyTracker) -> Result<()> {
    info!("[Bybit] connecting to {WS_URL}");
    let ws_stream = connect_ws(WS_URL).await?;
    info!("[Bybit] connected");
    let (mut write, mut read) = ws_stream.split();

    let args: Vec<String> = ALL_SYMBOLS
        .iter()
        .map(|s| format!("orderbook.5.{}", symbol_to_pair(s)))
        .collect();
    let sub = serde_json::json!({"op": "subscribe", "args": args});
    write.send(Message::Text(sub.to_string())).await?;
    info!("[Bybit] subscribed to orderbook.5");

    let ping_interval = tokio::time::interval(std::time::Duration::from_secs(20));
    tokio::pin!(ping_interval);
    let mut ping_sent_at: Option<Instant> = None;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if text.contains("\"pong\"") {
                            if let Some(sent) = ping_sent_at.take() {
                                let rtt = sent.elapsed().as_secs_f64() * 1000.0;
                                tracker.record(Exchange::Bybit, rtt);
                            }
                        } else if let Err(e) = handle_message(&text, tx, ob_tx) {
                            warn!("[Bybit] parse error: {e}");
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        if let Some(sent) = ping_sent_at.take() {
                            let rtt = sent.elapsed().as_secs_f64() * 1000.0;
                            tracker.record(Exchange::Bybit, rtt);
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        warn!("[Bybit] server closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("[Bybit] ws error: {e}");
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                ping_sent_at = Some(Instant::now());
                let ping = serde_json::json!({"op": "ping"});
                if let Err(e) = write.send(Message::Text(ping.to_string())).await {
                    error!("[Bybit] ping failed: {e}");
                    break;
                }
            }
        }
    }
    Ok(())
}

fn parse_levels(raw: &[[String; 2]], limit: usize) -> Vec<PriceLevel> {
    raw.iter().take(limit).filter_map(|l| {
        Some(PriceLevel {
            price: Decimal::from_str(&l[0]).ok()?,
            qty: Decimal::from_str(&l[1]).ok()?,
        })
    }).collect()
}

fn handle_message(text: &str, tx: &TickerSender, ob_tx: &OrderBookSender) -> Result<()> {
    let resp: WsResponse = serde_json::from_str(text)?;

    let topic = match resp.topic {
        Some(t) => t,
        None => return Ok(()),
    };
    if !topic.starts_with("orderbook.") {
        return Ok(());
    }

    let data = resp.data.ok_or_else(|| anyhow::anyhow!("missing data"))?;
    let symbol = pair_to_symbol(&data.s).ok_or_else(|| anyhow::anyhow!("unknown: {}", data.s))?;

    if data.b.is_empty() || data.a.is_empty() {
        return Ok(());
    }

    let now = Utc::now();

    // Emit ticker from best level
    let ticker = Ticker {
        exchange: Exchange::Bybit,
        symbol,
        best_bid: Decimal::from_str(&data.b[0][0])?,
        best_bid_qty: Decimal::from_str(&data.b[0][1])?,
        best_ask: Decimal::from_str(&data.a[0][0])?,
        best_ask_qty: Decimal::from_str(&data.a[0][1])?,
        quote_currency: QuoteCurrency::USDT,
        timestamp: now,
        local_timestamp: now,
    };
    let _ = tx.send(ticker);

    // Emit full orderbook
    let _ = ob_tx.send(OrderBookUpdate {
        exchange: Exchange::Bybit,
        symbol,
        bids: parse_levels(&data.b, 5),
        asks: parse_levels(&data.a, 5),
        quote_currency: QuoteCurrency::USDT,
        timestamp: now,
    });

    Ok(())
}
