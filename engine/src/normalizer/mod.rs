use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::models::*;

const USDT_USD_STALE_SECS: i64 = 30;
const KRW_USD_STALE_SECS: i64 = 1800; // Frankfurter refreshes every 600s, allow 30min staleness

pub struct RateManager {
    // KRW/USD: from Frankfurter API (ECB data)
    krw_per_usd: RwLock<Option<(Decimal, chrono::DateTime<Utc>)>>,
    // USDT/USD: from Binance USDC/USDT pair (USD value of 1 USDT)
    usd_per_usdt: RwLock<Option<(Decimal, chrono::DateTime<Utc>)>>,
    // KRW/USDT: derived from KRW/USD and USDT/USD (for backward compat with frontend)
    // Fallback: BTC implied rate when Frankfurter is unavailable
    implied_krw_per_usdt: RwLock<Option<(Decimal, chrono::DateTime<Utc>)>>,
}

impl RateManager {
    pub fn new() -> Self {
        Self {
            krw_per_usd: RwLock::new(None),
            usd_per_usdt: RwLock::new(None),
            implied_krw_per_usdt: RwLock::new(None),
        }
    }

    /// Called from Frankfurter API polling task
    pub fn update_krw_usd(&self, krw_per_usd: Decimal) {
        info!("KRW/USD rate updated (Frankfurter): {krw_per_usd}");
        *self.krw_per_usd.write() = Some((krw_per_usd, Utc::now()));
    }

    /// Called from Binance feed when USDC/USDT tick arrives
    /// Stores USD value of 1 USDT (= 1/USDC_USDT_mid)
    pub fn update_usdt_usd_rate(&self, usd_per_usdt: Decimal) {
        debug!("USD/USDT rate: {usd_per_usdt}");
        *self.usd_per_usdt.write() = Some((usd_per_usdt, Utc::now()));
    }

    /// Fallback: called from normalizer when BTC cross-price is available
    pub fn update_implied_krw_usdt(&self, krw_mid: Decimal, usdt_mid: Decimal) {
        if usdt_mid.is_zero() {
            return;
        }
        let rate = krw_mid / usdt_mid;
        debug!("implied KRW/USDT fallback rate: {rate}");
        *self.implied_krw_per_usdt.write() = Some((rate, Utc::now()));
    }

    /// Get KRW/USD from Frankfurter (ECB official rate)
    pub fn get_krw_per_usd(&self) -> Option<Decimal> {
        let guard = self.krw_per_usd.read();
        let (rate, ts) = guard.as_ref()?;
        let age = (Utc::now() - *ts).num_seconds();
        if age > KRW_USD_STALE_SECS {
            warn!("KRW/USD rate stale ({age}s old)");
            return None;
        }
        Some(*rate)
    }

    /// Get USD value of 1 USDT (from Binance USDC/USDT). Defaults to 1.0 if unavailable.
    pub fn get_usd_per_usdt(&self) -> Decimal {
        let guard = self.usd_per_usdt.read();
        match guard.as_ref() {
            Some((rate, ts)) => {
                let age = (Utc::now() - *ts).num_seconds();
                if age > USDT_USD_STALE_SECS {
                    warn!("USDT/USD rate stale ({age}s old), using 1.0");
                    Decimal::ONE
                } else {
                    *rate
                }
            }
            None => Decimal::ONE,
        }
    }

    /// Get KRW/USDT rate:
    /// Primary: derived from Frankfurter KRW/USD and WS USDT/USD
    /// Fallback: BTC implied cross-price
    pub fn get_krw_per_usdt(&self) -> Option<Decimal> {
        // Primary: KRW/USDT = KRW/USD / USD_per_USDT
        if let Some(krw_usd) = self.get_krw_per_usd() {
            let usd_per_usdt = self.get_usd_per_usdt();
            // 1 USDT = usd_per_usdt USD = usd_per_usdt * krw_usd KRW
            let krw_per_usdt = krw_usd * usd_per_usdt;
            return Some(krw_per_usdt);
        }

        // Fallback: BTC implied rate
        let guard = self.implied_krw_per_usdt.read();
        if let Some((rate, ts)) = guard.as_ref() {
            let age = (Utc::now() - *ts).num_seconds();
            if age <= 30 {
                warn!("using BTC implied KRW/USDT fallback: {rate}");
                return Some(*rate);
            }
        }
        None
    }

    /// Build the composite ExchangeRate for broadcasting
    pub fn get_rate(&self) -> Option<ExchangeRate> {
        let krw_per_usdt = self.get_krw_per_usdt()?;
        let usd_per_usdt = self.get_usd_per_usdt();
        // KRW/USD = KRW/USDT / USD_per_USDT
        let krw_per_usd = krw_per_usdt / usd_per_usdt;

        let source = if self.get_krw_per_usd().is_some() {
            RateSource::Cryprice
        } else {
            RateSource::Implied
        };

        Some(ExchangeRate {
            krw_per_usdt,
            usdt_per_usd: usd_per_usdt,
            krw_per_usd,
            source,
            timestamp: Utc::now(),
        })
    }
}

/// Fetch KRW/USD rate from Cryprice (scolkg.com) via Socket.IO HTTP polling.
/// Returns (krw_per_usd, usdt_krw) from the live crypto exchange rate service.
async fn fetch_cryprice_rates(client: &reqwest::Client) -> anyhow::Result<(Decimal, Option<Decimal>)> {
    const BASE: &str = "https://ticker1.cryprice.com/socket.io/";

    // Step 1: EIO4 handshake
    let handshake: String = client
        .get(format!("{BASE}?EIO=4&transport=polling"))
        .send().await?.text().await?;
    // Response: 0{"sid":"...","upgrades":[...],...}
    let json_start = handshake.find('{').ok_or_else(|| anyhow::anyhow!("bad handshake"))?;
    let hs: serde_json::Value = serde_json::from_str(&handshake[json_start..])?;
    let sid = hs["sid"].as_str().ok_or_else(|| anyhow::anyhow!("no sid"))?;

    let mut krw_per_usd: Option<Decimal> = None;
    let mut usdt_krw: Option<Decimal> = None;

    // Step 2: Connect to default namespace
    client.post(format!("{BASE}?EIO=4&transport=polling&sid={sid}"))
        .header("Content-Type", "text/plain;charset=UTF-8")
        .body("40")
        .send().await?;

    // Poll for connect ACK (also check for rate data in initial push)
    let ack = client.get(format!("{BASE}?EIO=4&transport=polling&sid={sid}"))
        .send().await?.text().await?;
    parse_cryprice_response(&ack, &mut krw_per_usd, &mut usdt_krw);

    // Step 3: Request rates
    client.post(format!("{BASE}?EIO=4&transport=polling&sid={sid}"))
        .header("Content-Type", "text/plain;charset=UTF-8")
        .body("42[\"request exchangerate usd krw\"]")
        .send().await?;

    // Step 4: Poll for responses (rate may arrive in the same or next poll)
    for _ in 0..4 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let resp = client.get(format!("{BASE}?EIO=4&transport=polling&sid={sid}"))
            .send().await?.text().await?;
        parse_cryprice_response(&resp, &mut krw_per_usd, &mut usdt_krw);
        if krw_per_usd.is_some() {
            break;
        }
        // Retry request in case it was missed
        let _ = client.post(format!("{BASE}?EIO=4&transport=polling&sid={sid}"))
            .header("Content-Type", "text/plain;charset=UTF-8")
            .body("42[\"request exchangerate usd krw\"]")
            .send().await;
    }

    // Close the connection gracefully
    let _ = client.post(format!("{BASE}?EIO=4&transport=polling&sid={sid}"))
        .header("Content-Type", "text/plain;charset=UTF-8")
        .body("1")
        .send().await;

    krw_per_usd.map(|r| (r, usdt_krw))
        .ok_or_else(|| anyhow::anyhow!("KRW/USD not found in cryprice response"))
}

fn parse_cryprice_response(resp: &str, krw_per_usd: &mut Option<Decimal>, usdt_krw: &mut Option<Decimal>) {
    for segment in resp.split("42[") {
        if segment.contains("exchangerateUsdKrw response") {
            if let Some(start) = segment.rfind(',') {
                let num_str = segment[start+1..].trim_end_matches(|c: char| c == ']' || c.is_whitespace());
                if let Ok(v) = Decimal::from_str(num_str) {
                    *krw_per_usd = Some(v);
                }
            }
        }
        if segment.contains("usdtprice response") {
            if let Some(start) = segment.rfind(',') {
                let num_str = segment[start+1..].trim_end_matches(|c: char| c == ']' || c.is_whitespace());
                if let Ok(v) = Decimal::from_str(num_str) {
                    *usdt_krw = Some(v);
                }
            }
        }
    }
}

/// Spawn a background task that polls Cryprice for KRW/USD rate
pub fn spawn_cryprice_poller(rate_manager: Arc<RateManager>, interval_secs: u64) {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        let mut first = true;
        loop {
            match fetch_cryprice_rates(&client).await {
                Ok((krw_usd, usdt_krw)) => {
                    rate_manager.update_krw_usd(krw_usd);
                    if first {
                        info!("Cryprice KRW/USD initial rate: {krw_usd}");
                        if let Some(uk) = usdt_krw {
                            info!("Cryprice USDT/KRW: {uk}");
                        }
                        first = false;
                    } else {
                        debug!("Cryprice KRW/USD: {krw_usd}");
                    }
                }
                Err(e) => {
                    error!("Cryprice rate fetch error: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
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
                let usd_per_usdt = self.rate_manager.get_usd_per_usdt();
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
                let usd_per_usdt = self.rate_manager.get_usd_per_usdt();
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
