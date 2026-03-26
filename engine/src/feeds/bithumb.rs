use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::models::{Exchange, OrderBookUpdate, PriceLevel, QuoteCurrency, Symbol, Ticker, ALL_SYMBOLS};

use super::connection::connect_ws;
use super::latency::LatencyTracker;
use super::{OrderBookSender, TickerSender};

const WS_URL: &str = "wss://pubwss.bithumb.com/pub/ws";

#[derive(Debug, Deserialize)]
struct WsResponse {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    content: Option<OrderbookContent>,
    status: Option<String>,
    resmsg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrderbookContent {
    list: Vec<OrderEntry>,
}

#[derive(Debug, Deserialize)]
struct OrderEntry {
    symbol: String,
    #[serde(rename = "orderType")]
    order_type: String,
    price: String,
    quantity: String,
}

fn symbol_to_pair(s: &Symbol) -> &'static str {
    match s {
        Symbol::BTC => "BTC_KRW",
        Symbol::ETH => "ETH_KRW",
        Symbol::SOL => "SOL_KRW",
        Symbol::XRP => "XRP_KRW",
    }
}

fn pair_to_symbol(pair: &str) -> Option<Symbol> {
    match pair {
        "BTC_KRW" => Some(Symbol::BTC),
        "ETH_KRW" => Some(Symbol::ETH),
        "SOL_KRW" => Some(Symbol::SOL),
        "XRP_KRW" => Some(Symbol::XRP),
        _ => None,
    }
}

// Local orderbook maintained per symbol
struct LocalBook {
    bids: BTreeMap<Decimal, Decimal>, // price → qty, descending iteration
    asks: BTreeMap<Decimal, Decimal>, // price → qty, ascending iteration
}

impl LocalBook {
    fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    fn update_bid(&mut self, price: Decimal, qty: Decimal) {
        if qty.is_zero() {
            self.bids.remove(&price);
        } else {
            self.bids.insert(price, qty);
        }
    }

    fn update_ask(&mut self, price: Decimal, qty: Decimal) {
        if qty.is_zero() {
            self.asks.remove(&price);
        } else {
            self.asks.insert(price, qty);
        }
    }

    fn top_bids(&self, n: usize) -> Vec<PriceLevel> {
        self.bids.iter().rev().take(n).map(|(p, q)| PriceLevel { price: *p, qty: *q }).collect()
    }

    fn top_asks(&self, n: usize) -> Vec<PriceLevel> {
        self.asks.iter().take(n).map(|(p, q)| PriceLevel { price: *p, qty: *q }).collect()
    }
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

    let symbols: Vec<String> = ALL_SYMBOLS.iter().map(|s| symbol_to_pair(s).to_string()).collect();
    let sub = serde_json::json!({"type": "orderbookdepth", "symbols": symbols});
    write.send(Message::Text(sub.to_string())).await?;
    info!("[Bithumb] subscribed to orderbookdepth");

    let mut books: std::collections::HashMap<String, LocalBook> = std::collections::HashMap::new();

    let ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    tokio::pin!(ping_interval);
    let mut ping_sent_at: Option<Instant> = None;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_message(&text, tx, ob_tx, &mut books) {
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

fn handle_message(
    text: &str,
    tx: &TickerSender,
    ob_tx: &OrderBookSender,
    books: &mut std::collections::HashMap<String, LocalBook>,
) -> Result<()> {
    let resp: WsResponse = serde_json::from_str(text)?;

    if resp.status.is_some() {
        let status = resp.status.as_deref().unwrap_or("");
        let msg = resp.resmsg.as_deref().unwrap_or("");
        info!("[Bithumb] status: {status} - {msg}");
        return Ok(());
    }

    if resp.msg_type.as_deref() != Some("orderbookdepth") {
        return Ok(());
    }

    let content = resp.content.ok_or_else(|| anyhow::anyhow!("missing content"))?;

    // Collect which symbols were updated
    let mut updated_symbols: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in &content.list {
        let price = Decimal::from_str(&entry.price)?;
        let qty = Decimal::from_str(&entry.quantity)?;
        let book = books.entry(entry.symbol.clone()).or_insert_with(LocalBook::new);

        match entry.order_type.as_str() {
            "bid" => book.update_bid(price, qty),
            "ask" => book.update_ask(price, qty),
            _ => continue,
        }
        updated_symbols.insert(entry.symbol.clone());
    }

    let now = Utc::now();
    for sym_str in &updated_symbols {
        let symbol = match pair_to_symbol(sym_str) {
            Some(s) => s,
            None => continue,
        };
        let book = match books.get(sym_str) {
            Some(b) => b,
            None => continue,
        };

        let top_bids = book.top_bids(5);
        let top_asks = book.top_asks(5);

        if top_bids.is_empty() || top_asks.is_empty() {
            continue;
        }

        // Emit ticker from best level
        let ticker = Ticker {
            exchange: Exchange::Bithumb,
            symbol,
            best_bid: top_bids[0].price,
            best_bid_qty: top_bids[0].qty,
            best_ask: top_asks[0].price,
            best_ask_qty: top_asks[0].qty,
            quote_currency: QuoteCurrency::KRW,
            timestamp: now,
            local_timestamp: now,
        };
        let _ = tx.send(ticker);

        // Emit orderbook
        let _ = ob_tx.send(OrderBookUpdate {
            exchange: Exchange::Bithumb,
            symbol,
            bids: top_bids,
            asks: top_asks,
            quote_currency: QuoteCurrency::KRW,
            timestamp: now,
        });
    }

    Ok(())
}
