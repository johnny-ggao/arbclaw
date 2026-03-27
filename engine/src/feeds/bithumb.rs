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

const WS_URL: &str = "wss://ws-api.bithumb.com/websocket/v1";

fn symbol_to_code(s: &Symbol) -> &'static str {
    match s {
        Symbol::BTC => "KRW-BTC",
        Symbol::ETH => "KRW-ETH",
        Symbol::SOL => "KRW-SOL",
        Symbol::XRP => "KRW-XRP",
    }
}

fn code_to_symbol(code: &str) -> Option<Symbol> {
    match code {
        "KRW-BTC" => Some(Symbol::BTC),
        "KRW-ETH" => Some(Symbol::ETH),
        "KRW-SOL" => Some(Symbol::SOL),
        "KRW-XRP" => Some(Symbol::XRP),
        _ => None,
    }
}

// v2 orderbook response format
#[derive(Debug, Deserialize)]
struct OrderbookMsg {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    code: Option<String>,
    orderbook_units: Option<Vec<OrderbookUnit>>,
    // v1 status fields
    status: Option<String>,
    resmsg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrderbookUnit {
    ask_price: f64,
    bid_price: f64,
    ask_size: f64,
    bid_size: f64,
}

pub async fn run(tx: TickerSender, ob_tx: OrderBookSender, tracker: Arc<LatencyTracker>) -> Result<()> {
    loop {
        if let Err(e) = connect_and_stream(&tx, &ob_tx, &tracker).await {
            error!("[Bithumb] connection error: {e}, reconnecting in 3s...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn connect_and_stream(tx: &TickerSender, ob_tx: &OrderBookSender, tracker: &LatencyTracker) -> Result<()> {
    info!("[Bithumb] connecting to {WS_URL}");
    let ws_stream = connect_ws(WS_URL).await?;
    info!("[Bithumb] connected");
    let (mut write, mut read) = ws_stream.split();

    // v2 orderbook subscription: snapshot + realtime (both by default)
    let codes: Vec<String> = ALL_SYMBOLS.iter().map(|s| symbol_to_code(s).to_string()).collect();
    let sub = serde_json::json!([
        {"ticket": "arbclaw"},
        {"type": "orderbook", "codes": codes},
        {"format": "DEFAULT"}
    ]);
    write.send(Message::Text(sub.to_string())).await?;
    info!("[Bithumb] subscribed to v2 orderbook");

    let ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    tokio::pin!(ping_interval);
    let mut ping_sent_at: Option<Instant> = None;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_message(&text, tx, ob_tx) {
                            warn!("[Bithumb] parse error: {e}");
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        let text = match String::from_utf8(data.to_vec()) {
                            Ok(s) => s,
                            Err(_) => {
                                warn!("[Bithumb] non-UTF8 binary: {}B", data.len());
                                continue;
                            }
                        };
                        if let Err(e) = handle_message(&text, tx, ob_tx) {
                            warn!("[Bithumb] parse error: {e}");
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        if let Some(sent) = ping_sent_at.take() {
                            let rtt = sent.elapsed().as_secs_f64() * 1000.0;
                            tracker.record(Exchange::Bithumb, rtt);
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        warn!("[Bithumb] server closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("[Bithumb] ws error: {e}");
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                ping_sent_at = Some(Instant::now());
                if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                    error!("[Bithumb] ping failed: {e}");
                    break;
                }
            }
        }
    }
    Ok(())
}

fn handle_message(text: &str, tx: &TickerSender, ob_tx: &OrderBookSender) -> Result<()> {
    let msg: OrderbookMsg = serde_json::from_str(text)?;

    // Handle v1 status messages (connection ack)
    if let Some(status) = &msg.status {
        let resmsg = msg.resmsg.as_deref().unwrap_or("");
        info!("[Bithumb] status: {status} - {resmsg}");
        return Ok(());
    }

    if msg.msg_type.as_deref() != Some("orderbook") {
        return Ok(());
    }

    let code = msg.code.as_deref().ok_or_else(|| anyhow::anyhow!("missing code"))?;
    let symbol = code_to_symbol(code).ok_or_else(|| anyhow::anyhow!("unknown code: {code}"))?;
    let units = msg.orderbook_units.ok_or_else(|| anyhow::anyhow!("missing orderbook_units"))?;

    if units.is_empty() {
        return Ok(());
    }

    // Each message is a full snapshot: units are sorted by proximity to mid price
    // units[0] has best bid and best ask
    let mut asks: Vec<PriceLevel> = Vec::new();
    let mut bids: Vec<PriceLevel> = Vec::new();

    for u in units.iter().take(5) {
        if u.ask_price > 0.0 && u.ask_size > 0.0 {
            asks.push(PriceLevel {
                price: Decimal::from_str(&format!("{:.0}", u.ask_price))?,
                qty: Decimal::from_str(&format!("{:.8}", u.ask_size))?,
            });
        }
        if u.bid_price > 0.0 && u.bid_size > 0.0 {
            bids.push(PriceLevel {
                price: Decimal::from_str(&format!("{:.0}", u.bid_price))?,
                qty: Decimal::from_str(&format!("{:.8}", u.bid_size))?,
            });
        }
    }

    if bids.is_empty() || asks.is_empty() {
        return Ok(());
    }

    let now = Utc::now();

    // Emit ticker from best level (index 0)
    let _ = tx.send(Ticker {
        exchange: Exchange::Bithumb,
        symbol,
        best_bid: bids[0].price,
        best_bid_qty: bids[0].qty,
        best_ask: asks[0].price,
        best_ask_qty: asks[0].qty,
        quote_currency: QuoteCurrency::KRW,
        timestamp: now,
        local_timestamp: now,
    });

    // Emit orderbook (asks sorted low→high, bids sorted high→low — already in correct order)
    let _ = ob_tx.send(OrderBookUpdate {
        exchange: Exchange::Bithumb,
        symbol,
        bids,
        asks,
        quote_currency: QuoteCurrency::KRW,
        timestamp: now,
    });

    Ok(())
}
