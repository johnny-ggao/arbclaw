use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::models::*;

const STALE_THRESHOLD_SECS: i64 = 30;

pub struct RateManager {
    implied_rate: RwLock<Option<ExchangeRate>>,
    external_rate: RwLock<Option<ExchangeRate>>,
}

impl RateManager {
    pub fn new() -> Self {
        Self {
            implied_rate: RwLock::new(None),
            external_rate: RwLock::new(None),
        }
    }

    pub fn update_implied_rate(&self, krw_mid: Decimal, usdt_mid: Decimal) {
        if usdt_mid.is_zero() {
            return;
        }
        let rate = krw_mid / usdt_mid;
        let er = ExchangeRate {
            krw_per_usd: rate,
            source: RateSource::Implied,
            timestamp: Utc::now(),
        };
        debug!("implied KRW/USD rate: {rate}");
        *self.implied_rate.write() = Some(er);
    }

    #[allow(dead_code)]
    pub fn update_external_rate(&self, rate: Decimal) {
        let er = ExchangeRate {
            krw_per_usd: rate,
            source: RateSource::External,
            timestamp: Utc::now(),
        };
        *self.external_rate.write() = Some(er);
    }

    pub fn get_rate(&self) -> Option<ExchangeRate> {
        let now = Utc::now();

        // Prefer external rate (no circular dependency)
        if let Some(ref rate) = *self.external_rate.read() {
            let age = (now - rate.timestamp).num_seconds();
            if age < STALE_THRESHOLD_SECS {
                return Some(rate.clone());
            }
        }

        // Fallback to implied rate
        if let Some(ref rate) = *self.implied_rate.read() {
            let age = (now - rate.timestamp).num_seconds();
            if age < STALE_THRESHOLD_SECS {
                return Some(rate.clone());
            } else {
                warn!("implied rate is stale ({age}s old)");
            }
        }

        None
    }
}

pub struct Normalizer {
    rate_manager: Arc<RateManager>,
    // Cache latest tickers per (exchange, symbol)
    latest_tickers: RwLock<HashMap<(Exchange, Symbol), Ticker>>,
}

impl Normalizer {
    pub fn new(rate_manager: Arc<RateManager>) -> Self {
        Self {
            rate_manager,
            latest_tickers: RwLock::new(HashMap::new()),
        }
    }

    pub fn process(&self, ticker: &Ticker) -> Option<NormalizedTicker> {
        // Cache the raw ticker
        self.latest_tickers
            .write()
            .insert((ticker.exchange, ticker.symbol), ticker.clone());

        // Update implied rate from BTC data
        if ticker.symbol == Symbol::BTC {
            self.try_update_implied_rate(ticker);
        }

        match ticker.quote_currency {
            QuoteCurrency::USDT => Some(NormalizedTicker {
                exchange: ticker.exchange,
                symbol: ticker.symbol,
                best_bid_usd: ticker.best_bid,
                best_bid_qty: ticker.best_bid_qty,
                best_ask_usd: ticker.best_ask,
                best_ask_qty: ticker.best_ask_qty,
                raw_bid: ticker.best_bid,
                raw_ask: ticker.best_ask,
                quote_currency: ticker.quote_currency,
                exchange_rate: None,
                timestamp: ticker.timestamp,
                local_timestamp: ticker.local_timestamp,
            }),
            QuoteCurrency::KRW => {
                let rate = self.rate_manager.get_rate()?;
                if rate.krw_per_usd.is_zero() {
                    return None;
                }
                Some(NormalizedTicker {
                    exchange: ticker.exchange,
                    symbol: ticker.symbol,
                    best_bid_usd: ticker.best_bid / rate.krw_per_usd,
                    best_bid_qty: ticker.best_bid_qty,
                    best_ask_usd: ticker.best_ask / rate.krw_per_usd,
                    best_ask_qty: ticker.best_ask_qty,
                    raw_bid: ticker.best_bid,
                    raw_ask: ticker.best_ask,
                    quote_currency: ticker.quote_currency,
                    exchange_rate: Some(rate.krw_per_usd),
                    timestamp: ticker.timestamp,
                    local_timestamp: ticker.local_timestamp,
                })
            }
        }
    }

    fn try_update_implied_rate(&self, ticker: &Ticker) {
        let tickers = self.latest_tickers.read();

        match ticker.exchange.quote_currency() {
            QuoteCurrency::KRW => {
                // We have KRW BTC price, look for USDT BTC price
                let krw_mid = (ticker.best_bid + ticker.best_ask) / Decimal::TWO;
                // Try Binance first, then Bybit
                for usdt_exchange in &[Exchange::Binance, Exchange::Bybit] {
                    if let Some(usdt_ticker) = tickers.get(&(*usdt_exchange, Symbol::BTC)) {
                        let usdt_mid =
                            (usdt_ticker.best_bid + usdt_ticker.best_ask) / Decimal::TWO;
                        self.rate_manager.update_implied_rate(krw_mid, usdt_mid);
                        return;
                    }
                }
            }
            QuoteCurrency::USDT => {
                let usdt_mid = (ticker.best_bid + ticker.best_ask) / Decimal::TWO;
                // Look for KRW BTC price from Upbit or Bithumb
                for krw_exchange in &[Exchange::Upbit, Exchange::Bithumb] {
                    if let Some(krw_ticker) = tickers.get(&(*krw_exchange, Symbol::BTC)) {
                        let krw_mid =
                            (krw_ticker.best_bid + krw_ticker.best_ask) / Decimal::TWO;
                        self.rate_manager.update_implied_rate(krw_mid, usdt_mid);
                        return;
                    }
                }
            }
        }
    }
}
