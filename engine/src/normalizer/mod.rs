use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::models::*;

const STALE_THRESHOLD_SECS: i64 = 30;

pub struct RateManager {
    // KRW/USDT: implied from BTC cross-price (Upbit KRW price / Binance USDT price)
    krw_per_usdt: RwLock<Option<(Decimal, chrono::DateTime<Utc>)>>,
    // USDT/USD: from Binance USDC/USDT pair (1/mid_price)
    usdt_per_usd: RwLock<Option<(Decimal, chrono::DateTime<Utc>)>>,
}

impl RateManager {
    pub fn new() -> Self {
        Self {
            krw_per_usdt: RwLock::new(None),
            usdt_per_usd: RwLock::new(None),
        }
    }

    /// Called from normalizer when BTC cross-price is available
    pub fn update_implied_krw_usdt(&self, krw_mid: Decimal, usdt_mid: Decimal) {
        if usdt_mid.is_zero() {
            return;
        }
        let rate = krw_mid / usdt_mid;
        debug!("implied KRW/USDT rate: {rate}");
        *self.krw_per_usdt.write() = Some((rate, Utc::now()));
    }

    /// Called from Binance feed when USDC/USDT tick arrives
    pub fn update_usdt_usd_rate(&self, usdt_per_usd: Decimal) {
        debug!("USDT/USD rate: {usdt_per_usd}");
        *self.usdt_per_usd.write() = Some((usdt_per_usd, Utc::now()));
    }

    /// Get KRW/USDT rate (for arbitrage calculation between USDT and KRW exchanges)
    pub fn get_krw_per_usdt(&self) -> Option<Decimal> {
        let guard = self.krw_per_usdt.read();
        let (rate, ts) = guard.as_ref()?;
        let age = (Utc::now() - *ts).num_seconds();
        if age > STALE_THRESHOLD_SECS {
            warn!("KRW/USDT rate stale ({age}s old)");
            return None;
        }
        Some(*rate)
    }

    /// Get USDT/USD rate (from Binance USDC/USDT). Defaults to 1.0 if not yet available.
    pub fn get_usdt_per_usd(&self) -> Decimal {
        let guard = self.usdt_per_usd.read();
        match guard.as_ref() {
            Some((rate, ts)) => {
                let age = (Utc::now() - *ts).num_seconds();
                if age > STALE_THRESHOLD_SECS {
                    warn!("USDT/USD rate stale ({age}s old), using 1.0");
                    Decimal::ONE
                } else {
                    *rate
                }
            }
            None => Decimal::ONE,
        }
    }

    /// Build the composite ExchangeRate for broadcasting
    pub fn get_rate(&self) -> Option<ExchangeRate> {
        let krw_per_usdt = self.get_krw_per_usdt()?;
        let usdt_per_usd = self.get_usdt_per_usd();
        // KRW/USD = KRW/USDT × USDT/USD
        // But we want: how many KRW per 1 USD
        // krw_per_usdt = KRW per 1 USDT
        // usdt_per_usd = USDT per 1 USD (i.e., 1/USDC_USDT_price, typically ~1.0005)
        // Wait: USDC/USDT mid = how many USDT for 1 USDC
        // If USDC=1USD, then mid = USDT_per_USD
        // So: 1 USD = mid USDT = mid * krw_per_usdt KRW
        // But we stored usdt_per_usd = 1/mid = USD per 1 USDT
        // So: krw_per_usd = krw_per_usdt / usdt_per_usd
        // Actually let me reconsider:
        // USDC/USDT mid = 0.9998 means 1 USDC costs 0.9998 USDT
        // USDC ≈ 1 USD, so 1 USD ≈ 0.9998 USDT
        // In Binance feed: usdt_per_usd = 1/mid = 1/0.9998 ≈ 1.0002 USD per USDT
        // That's actually USD_per_USDT, not USDT_per_USD. Let me fix naming.
        //
        // Correct interpretation:
        // USDC/USDT mid (e.g., 0.9998) = 1 USDC costs 0.9998 USDT
        // Since USDC ≈ USD: 1 USD = 0.9998 USDT
        // We stored: 1/mid = 1.0002 = how many USD you get for 1 USDT
        // So our stored value is USD_per_USDT = 1/mid
        //
        // KRW per USD = KRW_per_USDT * USDT_per_USD = krw_per_usdt * mid
        // = krw_per_usdt / (1/mid) = krw_per_usdt / usdt_per_usd
        //
        // But simpler: let's just store the raw mid as usdt_usd_mid
        // and compute: krw_per_usd = krw_per_usdt * (1 / usdt_per_usd)
        // Hmm, let me just be very clear:

        // usdt_per_usd field stores: 1/USDC_USDT_mid = USD value of 1 USDT
        // So: KRW per 1 USD = KRW_per_USDT / USD_per_USDT = krw_per_usdt / usdt_per_usd
        let krw_per_usd = krw_per_usdt / usdt_per_usd;

        Some(ExchangeRate {
            krw_per_usdt,
            usdt_per_usd,
            krw_per_usd,
            source: RateSource::Implied,
            timestamp: Utc::now(),
        })
    }
}

pub struct Normalizer {
    rate_manager: Arc<RateManager>,
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
        self.latest_tickers
            .write()
            .insert((ticker.exchange, ticker.symbol), ticker.clone());

        if ticker.symbol == Symbol::BTC {
            self.try_update_implied_rate(ticker);
        }

        match ticker.quote_currency {
            QuoteCurrency::USDT => {
                // Convert USDT to true USD using USDT/USD rate
                let usd_per_usdt = self.rate_manager.get_usdt_per_usd();
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
                let krw_per_usdt = self.rate_manager.get_krw_per_usdt()?;
                if krw_per_usdt.is_zero() {
                    return None;
                }
                let usd_per_usdt = self.rate_manager.get_usdt_per_usd();
                // KRW price → USDT → USD
                // price_usd = (price_krw / krw_per_usdt) * usd_per_usdt
                let bid_usd = (ticker.best_bid / krw_per_usdt) * usd_per_usdt;
                let ask_usd = (ticker.best_ask / krw_per_usdt) * usd_per_usdt;

                Some(NormalizedTicker {
                    exchange: ticker.exchange,
                    symbol: ticker.symbol,
                    best_bid_usd: bid_usd,
                    best_bid_qty: ticker.best_bid_qty,
                    best_ask_usd: ask_usd,
                    best_ask_qty: ticker.best_ask_qty,
                    raw_bid: ticker.best_bid,
                    raw_ask: ticker.best_ask,
                    quote_currency: ticker.quote_currency,
                    exchange_rate: Some(krw_per_usdt),
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
                let krw_mid = (ticker.best_bid + ticker.best_ask) / Decimal::TWO;
                for usdt_exchange in &[Exchange::Binance, Exchange::Bybit] {
                    if let Some(usdt_ticker) = tickers.get(&(*usdt_exchange, Symbol::BTC)) {
                        let usdt_mid =
                            (usdt_ticker.best_bid + usdt_ticker.best_ask) / Decimal::TWO;
                        self.rate_manager
                            .update_implied_krw_usdt(krw_mid, usdt_mid);
                        return;
                    }
                }
            }
            QuoteCurrency::USDT => {
                let usdt_mid = (ticker.best_bid + ticker.best_ask) / Decimal::TWO;
                for krw_exchange in &[Exchange::Upbit, Exchange::Bithumb] {
                    if let Some(krw_ticker) = tickers.get(&(*krw_exchange, Symbol::BTC)) {
                        let krw_mid =
                            (krw_ticker.best_bid + krw_ticker.best_ask) / Decimal::TWO;
                        self.rate_manager
                            .update_implied_krw_usdt(krw_mid, usdt_mid);
                        return;
                    }
                }
            }
        }
    }
}
