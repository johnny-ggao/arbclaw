use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
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
    #[serde(rename = "type")]
    msg_type: Option<String>,
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

use std::collections::BTreeMap;

struct LocalBook {
    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,
}

impl LocalBook {
    fn new() -> Self {
        Self { bids: BTreeMap::new(), asks: BTreeMap::new() }
    }

    fn apply_snapshot(&mut self, bids: &[[String; 2]], asks: &[[String; 2]]) {
        self.bids.clear();
        self.asks.clear();
        for b in bids {
            if let (Ok(p), Ok(q)) = (Decimal::from_str(&b[0]), Decimal::from_str(&b[1])) {
                if !q.is_zero() { self.bids.insert(p, q); }
            }
        }
        for a in asks {
            if let (Ok(p), Ok(q)) = (Decimal::from_str(&a[0]), Decimal::from_str(&a[1])) {
                if !q.is_zero() { self.asks.insert(p, q); }
            }
        }
    }

    fn apply_delta(&mut self, bids: &[[String; 2]], asks: &[[String; 2]]) {
        for b in bids {
            if let (Ok(p), Ok(q)) = (Decimal::from_str(&b[0]), Decimal::from_str(&b[1])) {
                if q.is_zero() { self.bids.remove(&p); } else { self.bids.insert(p, q); }
            }
        }
        for a in asks {
            if let (Ok(p), Ok(q)) = (Decimal::from_str(&a[0]), Decimal::from_str(&a[1])) {
                if q.is_zero() { self.asks.remove(&p); } else { self.asks.insert(p, q); }
            }
        }
    }

    fn top_bids(&self, n: usize) -> Vec<PriceLevel> {
        self.bids.iter().rev().take(n).map(|(&p, &q)| PriceLevel { price: p, qty: q }).collect()
    }

    fn top_asks(&self, n: usize) -> Vec<PriceLevel> {
        self.asks.iter().take(n).map(|(&p, &q)| PriceLevel { price: p, qty: q }).collect()
    }
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

    // Spot only supports depths: 1, 50, 200, 1000. Use 50 and take top 5.
    let args: Vec<String> = ALL_SYMBOLS
        .iter()
        .map(|s| format!("orderbook.50.{}", symbol_to_pair(s)))
        .collect();
    let sub = serde_json::json!({"op": "subscribe", "args": args});
    write.send(Message::Text(sub.to_string())).await?;
    info!("[Bybit] subscribed to orderbook.50");

    let books: Arc<Mutex<HashMap<Symbol, LocalBook>>> = Arc::new(Mutex::new(HashMap::new()));

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
                        } else if let Err(e) = handle_message(&text, tx, ob_tx, &books) {
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

fn handle_message(
    text: &str,
    tx: &TickerSender,
    ob_tx: &OrderBookSender,
    books: &Arc<Mutex<HashMap<Symbol, LocalBook>>>,
) -> Result<()> {
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
    let msg_type = resp.msg_type.unwrap_or_default();

    let mut books_guard = books.lock();
    let book = books_guard.entry(symbol).or_insert_with(LocalBook::new);

    match msg_type.as_str() {
        "snapshot" => book.apply_snapshot(&data.b, &data.a),
        "delta" => book.apply_delta(&data.b, &data.a),
        _ => return Ok(()),
    }

    let top_bids = book.top_bids(5);
    let top_asks = book.top_asks(5);
    drop(books_guard);

    if top_bids.is_empty() || top_asks.is_empty() {
        return Ok(());
    }

    let now = Utc::now();

    let ticker = Ticker {
        exchange: Exchange::Bybit,
        symbol,
        best_bid: top_bids[0].price,
        best_bid_qty: top_bids[0].qty,
        best_ask: top_asks[0].price,
        best_ask_qty: top_asks[0].qty,
        quote_currency: QuoteCurrency::USDT,
        timestamp: now,
        local_timestamp: now,
    };
    let _ = tx.send(ticker);

    let _ = ob_tx.send(OrderBookUpdate {
        exchange: Exchange::Bybit,
        symbol,
        bids: top_bids,
        asks: top_asks,
        quote_currency: QuoteCurrency::USDT,
        timestamp: now,
    });

    Ok(())
}
