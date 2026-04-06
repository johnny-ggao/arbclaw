//! Runtime configuration (environment variables).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KrwUsdSource {
    Yahoo,
    Bok,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsdtUsdExchange {
    Binance,
    Bybit,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Yahoo Finance real-time KRW per 1 USD or BOK ECOS daily series.
    pub krw_usd_source: KrwUsdSource,
    /// How often to refresh KRW/USD from REST (default 60s).
    pub krw_usd_refresh_secs: u64,
    /// If REST data is older than this, `get_rate` / KRW normalization treat it as missing.
    pub krw_usd_stale_secs: i64,
    /// Which venue supplies USDC/USDT (or configured pair) bookTicker for USDT→USD.
    pub usdt_usd_exchange: UsdtUsdExchange,
    /// e.g. USDCUSDT — mid price → USD per USDT via 1/mid (same as existing Binance logic).
    pub usdt_usd_pair: String,
    /// Max age for WS-derived USDT/USD before falling back to 1.0 (default 30s).
    pub usdt_usd_stale_secs: i64,
    /// BOK ECOS API key (`BOK_ECOS_API_KEY`) when `krw_usd_source == Bok`.
    pub bok_api_key: Option<String>,
}

impl EngineConfig {
    pub fn from_env() -> Self {
        let krw_usd_source = match std::env::var("ARBC_KRW_USD_SOURCE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "bok" => KrwUsdSource::Bok,
            _ => KrwUsdSource::Yahoo,
        };

        let krw_usd_refresh_secs = std::env::var("ARBC_KRW_USD_REFRESH_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);

        let krw_usd_stale_secs = std::env::var("ARBC_KRW_USD_STALE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7200);

        let usdt_usd_exchange = match std::env::var("ARBC_USDT_USD_EXCHANGE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "bybit" => UsdtUsdExchange::Bybit,
            _ => UsdtUsdExchange::Binance,
        };

        let usdt_usd_pair = std::env::var("ARBC_USDT_USD_PAIR").unwrap_or_else(|_| "USDCUSDT".to_string());
        let usdt_usd_pair = usdt_usd_pair.trim().to_uppercase();
        if usdt_usd_pair.is_empty() {
            panic!("ARBC_USDT_USD_PAIR must not be empty");
        }

        let usdt_usd_stale_secs = std::env::var("ARBC_USDT_USD_STALE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let bok_api_key = std::env::var("BOK_ECOS_API_KEY").ok().filter(|s| !s.is_empty());

        Self {
            krw_usd_source,
            krw_usd_refresh_secs,
            krw_usd_stale_secs,
            usdt_usd_exchange,
            usdt_usd_pair,
            usdt_usd_stale_secs,
            bok_api_key,
        }
    }

    pub fn usdt_pair_lower(&self) -> String {
        self.usdt_usd_pair.to_lowercase()
    }
}
