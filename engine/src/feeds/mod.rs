pub mod binance;
pub mod bybit;
pub mod upbit;
pub mod bithumb;
pub mod connection;
pub mod latency;

use crate::models::{Ticker, OrderBookUpdate};
use tokio::sync::broadcast;

pub type TickerSender = broadcast::Sender<Ticker>;
pub type OrderBookSender = broadcast::Sender<OrderBookUpdate>;
