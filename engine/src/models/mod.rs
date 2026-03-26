use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    Binance,
    Bybit,
    Upbit,
    Bithumb,
}

impl fmt::Display for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Exchange::Binance => write!(f, "Binance"),
            Exchange::Bybit => write!(f, "Bybit"),
            Exchange::Upbit => write!(f, "Upbit"),
            Exchange::Bithumb => write!(f, "Bithumb"),
        }
    }
}

impl Exchange {
    pub fn taker_fee(&self) -> Decimal {
        match self {
            Exchange::Binance | Exchange::Bybit => Decimal::new(10, 4), // 0.0010
            Exchange::Upbit | Exchange::Bithumb => Decimal::new(25, 4), // 0.0025
        }
    }

    pub fn quote_currency(&self) -> QuoteCurrency {
        match self {
            Exchange::Binance | Exchange::Bybit => QuoteCurrency::USDT,
            Exchange::Upbit | Exchange::Bithumb => QuoteCurrency::KRW,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Symbol {
    BTC,
    ETH,
    SOL,
    XRP,
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Symbol::BTC => write!(f, "BTC"),
            Symbol::ETH => write!(f, "ETH"),
            Symbol::SOL => write!(f, "SOL"),
            Symbol::XRP => write!(f, "XRP"),
        }
    }
}

pub const ALL_SYMBOLS: [Symbol; 4] = [Symbol::BTC, Symbol::ETH, Symbol::SOL, Symbol::XRP];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuoteCurrency {
    USDT,
    KRW,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub exchange: Exchange,
    pub symbol: Symbol,
    pub best_bid: Decimal,
    pub best_bid_qty: Decimal,
    pub best_ask: Decimal,
    pub best_ask_qty: Decimal,
    pub quote_currency: QuoteCurrency,
    pub timestamp: DateTime<Utc>,
    pub local_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedTicker {
    pub exchange: Exchange,
    pub symbol: Symbol,
    pub best_bid_usd: Decimal,
    pub best_bid_qty: Decimal,
    pub best_ask_usd: Decimal,
    pub best_ask_qty: Decimal,
    pub raw_bid: Decimal,
    pub raw_ask: Decimal,
    pub quote_currency: QuoteCurrency,
    pub exchange_rate: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
    pub local_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub krw_per_usdt: Decimal,
    pub usdt_per_usd: Decimal,
    pub krw_per_usd: Decimal,
    pub source: RateSource,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateSource {
    Implied,
    Cryprice,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageSignal {
    pub buy_exchange: Exchange,
    pub sell_exchange: Exchange,
    pub symbol: Symbol,
    pub gross_spread_pct: Decimal,
    pub net_spread_pct: Decimal,
    pub max_qty: Decimal,
    pub estimated_profit_usd: Decimal,
    pub buy_price_usd: Decimal,
    pub sell_price_usd: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedStatus {
    pub exchange: Exchange,
    pub connected: bool,
    pub last_update: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyReport {
    pub exchanges: Vec<ExchangeLatency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeLatency {
    pub exchange: String,
    pub last_rtt_ms: f64,
    pub avg_rtt_ms: f64,
    pub min_rtt_ms: f64,
    pub max_rtt_ms: f64,
    pub samples: usize,
}

// Order book depth (up to 5 levels)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Decimal,
    pub qty: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookUpdate {
    pub exchange: Exchange,
    pub symbol: Symbol,
    pub bids: Vec<PriceLevel>, // sorted high→low
    pub asks: Vec<PriceLevel>, // sorted low→high
    pub quote_currency: QuoteCurrency,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    #[serde(rename = "ticker")]
    Ticker(NormalizedTicker),
    #[serde(rename = "signal")]
    Signal(ArbitrageSignal),
    #[serde(rename = "rate")]
    Rate(ExchangeRate),
    #[serde(rename = "status")]
    Status(FeedStatus),
    #[serde(rename = "latency")]
    Latency(LatencyReport),
    #[serde(rename = "orderbook")]
    OrderBook(OrderBookUpdate),
}
