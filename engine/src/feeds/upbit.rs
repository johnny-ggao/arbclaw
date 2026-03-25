use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::models::{Exchange, QuoteCurrency, Symbol, Ticker, ALL_SYMBOLS};

use super::TickerSender;
use super::connection::connect_ws;
use super::latency::LatencyTracker;

const WS_URL: &str = "wss://api.upbit.com/websocket/v1";

#[derive(Debug, Deserialize)]
struct OrderbookResponse {
    code: Option<String>,
    cd: Option<String>,
    orderbook_units: Option<Vec<OrderbookUnit>>,
    obu: Option<Vec<OrderbookUnit>>,
}

#[derive(Debug, Deserialize)]
struct OrderbookUnit {
    #[serde(alias = "ap")]
    ask_price: Decimal,
    #[serde(alias = "as")]
    ask_size: Decimal,
    #[serde(alias = "bp")]
    bid_price: Decimal,
    #[serde(alias = "bs")]
    bid_size: Decimal,
}

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

pub async fn run(tx: TickerSender, tracker: Arc<LatencyTracker>) -> Result<()> {
    loop {
        if let Err(e) = connect_and_stream(&tx, &tracker).await {
            error!("[Upbit] connection error: {e}, reconnecting in 3s...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn connect_and_stream(tx: &TickerSender, tracker: &LatencyTracker) -> Result<()> {
    info!("[Upbit] connecting to {WS_URL}");
    let ws_stream = connect_ws(WS_URL).await?;
    info!("[Upbit] connected");
    let (mut write, mut read) = ws_stream.split();

    let codes: Vec<String> = ALL_SYMBOLS.iter().map(|s| symbol_to_code(s).to_string()).collect();
    let sub = serde_json::json!([
        {"ticket": "cex-arb"},
        {"type": "orderbook", "codes": codes}
    ]);
    write.send(Message::Text(sub.to_string())).await?;
    info!("[Upbit] subscribed to orderbook");

    let ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    tokio::pin!(ping_interval);
    let mut ping_sent_at: Option<Instant> = None;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_message(&text, tx) {
                            warn!("[Upbit] text parse error: {e}");
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        match String::from_utf8(data.to_vec()) {
                            Ok(text) => {
                                if let Err(e) = handle_message(&text, tx) {
                                    warn!("[Upbit] binary parse error: {e}");
                                }
                            }
                            Err(e) => warn!("[Upbit] utf8 error: {e}"),
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        if let Some(sent) = ping_sent_at.take() {
                            let rtt = sent.elapsed().as_secs_f64() * 1000.0;
                            tracker.record(Exchange::Upbit, rtt);
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        warn!("[Upbit] server closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("[Upbit] ws error: {e}");
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                ping_sent_at = Some(Instant::now());
                if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                    error!("[Upbit] ping failed: {e}");
                    break;
                }
            }
        }
    }
    Ok(())
}

fn handle_message(text: &str, tx: &TickerSender) -> Result<()> {
    let resp: OrderbookResponse = serde_json::from_str(text)?;

    let code = resp.cd.or(resp.code).ok_or_else(|| anyhow::anyhow!("missing code field"))?;
    let symbol = code_to_symbol(&code).ok_or_else(|| anyhow::anyhow!("unknown code: {code}"))?;

    let units = resp.obu.or(resp.orderbook_units)
        .ok_or_else(|| anyhow::anyhow!("missing orderbook_units"))?;

    if units.is_empty() {
        return Ok(());
    }

    let best = &units[0];
    let now = Utc::now();

    let ticker = Ticker {
        exchange: Exchange::Upbit,
        symbol,
        best_bid: best.bid_price,
        best_bid_qty: best.bid_size,
        best_ask: best.ask_price,
        best_ask_qty: best.ask_size,
        quote_currency: QuoteCurrency::KRW,
        timestamp: now,
        local_timestamp: now,
    };
    let _ = tx.send(ticker);
    Ok(())
}
