//! FX rates for USD normalization: KRW/USD (REST, cached) and USDT/USD (WS bookTicker).
//! Hot path uses `ArcSwapOption` loads (lock-free), not mutex-protected maps.

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use tracing::{debug, error, info, warn};

use crate::config::{EngineConfig, KrwUsdSource};
use crate::models::{ExchangeRate, RateSource};

// --- Snapshots (immutable, behind Arc) ------------------------------------

#[derive(Debug, Clone)]
struct KrwSnapshot {
    krw_per_usd: Decimal,
    updated_at: DateTime<Utc>,
    source: RateSource,
}

#[derive(Debug, Clone)]
struct UsdtSnapshot {
    /// USD value of 1 USDT (same semantics as previous `get_usd_per_usdt`).
    usd_per_usdt: Decimal,
    updated_at: DateTime<Utc>,
}

// --- RateManager ------------------------------------------------------------

pub struct RateManager {
    krw_usd: ArcSwapOption<KrwSnapshot>,
    usdt_usd: ArcSwapOption<UsdtSnapshot>,
    krw_stale_secs: i64,
    usdt_stale_secs: i64,
}

impl RateManager {
    pub fn new(cfg: &EngineConfig) -> Self {
        Self {
            krw_usd: ArcSwapOption::empty(),
            usdt_usd: ArcSwapOption::empty(),
            krw_stale_secs: cfg.krw_usd_stale_secs,
            usdt_stale_secs: cfg.usdt_usd_stale_secs,
        }
    }

    /// REST poller: KRW per 1 USD.
    pub fn store_krw_per_usd(&self, krw_per_usd: Decimal, source: RateSource) {
        info!("KRW/USD updated ({source}): {krw_per_usd}", source = source);
        self.krw_usd.store(Some(Arc::new(KrwSnapshot {
            krw_per_usd,
            updated_at: Utc::now(),
            source,
        })));
    }

    /// WS bookTicker: `mid` = mid price of configured USDC/USDT (or pair); USD per USDT = 1/mid.
    pub fn update_usdt_usd_from_pair_mid(&self, mid: Decimal) {
        if mid.is_zero() {
            return;
        }
        let usd_per_usdt = Decimal::ONE / mid;
        debug!("USDT/USD (USD per USDT): {usd_per_usdt}");
        self.usdt_usd.store(Some(Arc::new(UsdtSnapshot {
            usd_per_usdt,
            updated_at: Utc::now(),
        })));
    }

    /// Hot path: KRW per USD if fresh.
    pub fn krw_per_usd_live(&self) -> Option<Decimal> {
        let g = self.krw_usd.load();
        let s = g.as_ref()?;
        let age = (Utc::now() - s.updated_at).num_seconds();
        if age > self.krw_stale_secs {
            warn!("KRW/USD stale ({age}s > {})", self.krw_stale_secs);
            return None;
        }
        Some(s.krw_per_usd)
    }

    /// Hot path: USD per USDT; defaults to 1.0 when missing or stale.
    pub fn usd_per_usdt_live(&self) -> Decimal {
        let g = self.usdt_usd.load();
        match g.as_ref() {
            Some(s) => {
                let age = (Utc::now() - s.updated_at).num_seconds();
                if age > self.usdt_stale_secs {
                    warn!("USDT/USD stale ({age}s > {}), using 1.0", self.usdt_stale_secs);
                    Decimal::ONE
                } else {
                    s.usd_per_usdt
                }
            }
            None => Decimal::ONE,
        }
    }

    pub fn get_rate(&self) -> Option<ExchangeRate> {
        let g = self.krw_usd.load();
        let krw = g.as_ref()?;
        let age = (Utc::now() - krw.updated_at).num_seconds();
        if age > self.krw_stale_secs {
            return None;
        }

        let usd_per_usdt = self.usd_per_usdt_live();
        let krw_per_usd = krw.krw_per_usd;
        let krw_per_usdt = krw_per_usd * usd_per_usdt;

        Some(ExchangeRate {
            krw_per_usdt,
            usdt_per_usd: usd_per_usdt,
            krw_per_usd,
            source: krw.source,
            timestamp: Utc::now(),
        })
    }
}

// --- Yahoo Finance ----------------------------------------------------------

/// Fetch real-time KRW/USD from Yahoo Finance chart API (`KRW=X`).
pub async fn fetch_yahoo_krw_per_usd(client: &reqwest::Client) -> anyhow::Result<Decimal> {
    let url = "https://query1.finance.yahoo.com/v8/finance/chart/KRW=X?range=1d&interval=1d";
    let resp = client
        .get(url)
        .header("User-Agent", "arbclaw/1.0")
        .send()
        .await?
        .error_for_status()?;
    let body: YahooChartResponse = resp.json().await?;
    let result = body
        .chart
        .result
        .first()
        .ok_or_else(|| anyhow::anyhow!("Yahoo: empty chart result"))?;
    let price = result.meta.regular_market_price;
    if price < 500.0 || price > 5000.0 {
        return Err(anyhow::anyhow!("Yahoo: implausible KRW/USD rate: {price}"));
    }
    Decimal::from_str(&format!("{price:.4}"))
        .map_err(|e| anyhow::anyhow!("Yahoo KRW parse: {e}"))
}

#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Vec<YahooChartResult>,
}

#[derive(Debug, Deserialize)]
struct YahooChartResult {
    meta: YahooChartMeta,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooChartMeta {
    regular_market_price: f64,
}

// --- BOK ECOS ---------------------------------------------------------------

/// Daily table 731Y001 — major FX; uses last plausible `DATA_VALUE` in the returned row/list.
pub async fn fetch_bok_krw_per_usd(client: &reqwest::Client, api_key: &str) -> anyhow::Result<Decimal> {
    let date = Utc::now().format("%Y%m%d").to_string();
    let url = format!(
        "https://ecos.bok.or.kr/api/StatisticSearch/{api_key}/json/kr/1/1/731Y001/D/{date}/{date}/"
    );
    let text = client.get(&url).send().await?.error_for_status()?.text().await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!("BOK JSON parse: {e}; body={}", text.chars().take(200).collect::<String>())
    })?;

    let list: Vec<serde_json::Value> =
        if let Some(a) = v.pointer("/StatisticSearch/row").and_then(|x| x.as_array()) {
            a.clone()
        } else if let Some(a) = v.pointer("/StatisticSearch/list").and_then(|x| x.as_array()) {
            a.clone()
        } else if let Some(o) = v.pointer("/StatisticSearch/row").and_then(|x| x.as_object()) {
            vec![serde_json::Value::Object(o.clone())]
        } else {
            return Err(anyhow::anyhow!("BOK: missing StatisticSearch row/list"));
        };

    let mut last_ok: Option<Decimal> = None;
    for row in &list {
        let data_val = row.get("DATA_VALUE");
        let s = data_val
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .or_else(|| data_val.and_then(|x| x.as_f64()).map(|f| format!("{f}")));
        let Some(s) = s else { continue };
        if let Ok(d) = Decimal::from_str(s.trim()) {
            if d >= Decimal::new(500, 0) && d <= Decimal::new(5000, 0) {
                last_ok = Some(d);
            }
        }
    }

    last_ok.ok_or_else(|| anyhow::anyhow!("BOK: no usable DATA_VALUE in response"))
}

// --- Poller -----------------------------------------------------------------

pub fn spawn_krw_usd_poller(rate_manager: Arc<RateManager>, config: Arc<EngineConfig>) {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");

        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(config.krw_usd_refresh_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            // First `tick` completes immediately, then every `krw_usd_refresh_secs`.
            interval.tick().await;

            let result = match config.krw_usd_source {
                KrwUsdSource::Yahoo => fetch_yahoo_krw_per_usd(&client).await,
                KrwUsdSource::Bok => match &config.bok_api_key {
                    Some(key) => fetch_bok_krw_per_usd(&client, key.as_str()).await,
                    None => {
                        error!("ARBC_KRW_USD_SOURCE=bok but BOK_ECOS_API_KEY is not set");
                        Err(anyhow::anyhow!("missing BOK_ECOS_API_KEY"))
                    }
                },
            };

            match result {
                Ok(krw) => {
                    let src = match config.krw_usd_source {
                        KrwUsdSource::Yahoo => RateSource::Yahoo,
                        KrwUsdSource::Bok => RateSource::Bok,
                    };
                    rate_manager.store_krw_per_usd(krw, src);
                }
                Err(e) => error!("KRW/USD fetch failed: {e}"),
            }
        }
    });
}
