//! Normalizes venue quotes to USD using cached KRW/USD and live USDT/USD (see `crate::rates`).

use std::sync::Arc;

use crate::models::*;
use crate::rates::RateManager;

pub struct Normalizer {
    rate_manager: Arc<RateManager>,
}

impl Normalizer {
    pub fn new(rate_manager: Arc<RateManager>) -> Self {
        Self { rate_manager }
    }

    /// `price_in_usd_krw = price_krw / krw_usd`, `price_in_usd_usdt = price_usdt * usdt_usd`
    /// (`usdt_usd` here is USD per 1 USDT = `usd_per_usdt`).
    pub fn process(&self, ticker: &Ticker) -> Option<NormalizedTicker> {
        match ticker.quote_currency {
            QuoteCurrency::USDT => {
                let usd_per_usdt = self.rate_manager.usd_per_usdt_live();
                Some(NormalizedTicker {
                    exchange: ticker.exchange,
                    symbol: ticker.symbol,
                    best_bid_usd: ticker.best_bid * usd_per_usdt,
                    best_bid_qty: ticker.best_bid_qty,
                    best_ask_usd: ticker.best_ask * usd_per_usdt,
                    best_ask_qty: ticker.best_ask_qty,
                    raw_bid: ticker.best_bid,
                    raw_ask: ticker.best_ask,
                    quote_currency: ticker.quote_currency,
                    exchange_rate: None,
                    timestamp: ticker.timestamp,
                    local_timestamp: ticker.local_timestamp,
                })
            }
            QuoteCurrency::KRW => {
                let krw_per_usd = self.rate_manager.krw_per_usd_live()?;
                if krw_per_usd.is_zero() {
                    return None;
                }
                Some(NormalizedTicker {
                    exchange: ticker.exchange,
                    symbol: ticker.symbol,
                    best_bid_usd: ticker.best_bid / krw_per_usd,
                    best_bid_qty: ticker.best_bid_qty,
                    best_ask_usd: ticker.best_ask / krw_per_usd,
                    best_ask_qty: ticker.best_ask_qty,
                    raw_bid: ticker.best_bid,
                    raw_ask: ticker.best_ask,
                    quote_currency: ticker.quote_currency,
                    exchange_rate: Some(krw_per_usd),
                    timestamp: ticker.timestamp,
                    local_timestamp: ticker.local_timestamp,
                })
            }
        }
    }
}
