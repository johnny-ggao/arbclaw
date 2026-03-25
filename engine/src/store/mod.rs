use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

use crate::feeds::latency::LatencySnapshot;
use crate::models::{ArbitrageSignal, Exchange, ExchangeRate, NormalizedTicker, Symbol};

// ============================================================
// Memory Budget (approximate per-entry sizes)
// ============================================================
// StoredSignal:  ~160 bytes × 100,000 = ~16 MB
// TickerSnapshot: ~80 bytes × 172,800 = ~14 MB (16 pairs × 10,800 = 1/5s × 15h)
// RateEntry:      ~32 bytes ×  10,000 = ~0.3 MB
// LatencyTracker:  ~8 bytes ×     240 = negligible (in feeds/latency.rs)
// ----------------------------------------------------------
// Total hard cap: ~30 MB
// ============================================================

const MAX_SIGNALS: usize = 100_000;
const MAX_TICKER_PER_PAIR: usize = 10_800; // 1 sample/5s × 15 hours
const TICKER_SAMPLE_INTERVAL_MS: i64 = 5_000;
const MAX_RATES: usize = 10_000;

// ============================================================
// Stored types (compact f64 representation for memory efficiency)
// ============================================================

#[derive(Clone, Serialize)]
pub struct StoredSignal {
    pub buy_exchange: String,
    pub sell_exchange: String,
    pub symbol: String,
    pub net_spread_pct: f64,
    pub gross_spread_pct: f64,
    pub estimated_profit_usd: f64,
    pub max_qty: f64,
    pub buy_price_usd: f64,
    pub sell_price_usd: f64,
    pub timestamp: DateTime<Utc>,
}

impl From<&ArbitrageSignal> for StoredSignal {
    fn from(s: &ArbitrageSignal) -> Self {
        Self {
            buy_exchange: s.buy_exchange.to_string(),
            sell_exchange: s.sell_exchange.to_string(),
            symbol: s.symbol.to_string(),
            net_spread_pct: s.net_spread_pct.to_f64().unwrap_or(0.0),
            gross_spread_pct: s.gross_spread_pct.to_f64().unwrap_or(0.0),
            estimated_profit_usd: s.estimated_profit_usd.to_f64().unwrap_or(0.0),
            max_qty: s.max_qty.to_f64().unwrap_or(0.0),
            buy_price_usd: s.buy_price_usd.to_f64().unwrap_or(0.0),
            sell_price_usd: s.sell_price_usd.to_f64().unwrap_or(0.0),
            timestamp: s.timestamp,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct TickerSnapshot {
    pub bid_usd: f64,
    pub ask_usd: f64,
    pub raw_bid: f64,
    pub raw_ask: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
pub struct RateEntry {
    pub krw_per_usd: f64,
    pub source: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct MemoryUsage {
    pub signals_count: usize,
    pub signals_cap: usize,
    pub ticker_pairs: usize,
    pub ticker_total_entries: usize,
    pub ticker_cap_per_pair: usize,
    pub rates_count: usize,
    pub rates_cap: usize,
    pub estimated_mb: f64,
}

// ============================================================
// Snapshot returned on page load
// ============================================================

#[derive(Serialize)]
pub struct StateSnapshot {
    pub tickers: HashMap<String, TickerSnapshot>,
    pub rate: Option<RateEntry>,
    pub latency: Vec<LatencySnapshot>,
    pub recent_signals: Vec<StoredSignal>,
}

// ============================================================
// DataStore: unified, bounded storage
// ============================================================

pub struct DataStore {
    signals: RwLock<VecDeque<StoredSignal>>,
    tickers: RwLock<HashMap<(Exchange, Symbol), VecDeque<TickerSnapshot>>>,
    ticker_last_sample: RwLock<HashMap<(Exchange, Symbol), DateTime<Utc>>>,
    latest_tickers: RwLock<HashMap<(Exchange, Symbol), TickerSnapshot>>,
    rates: RwLock<VecDeque<RateEntry>>,
    latest_rate: RwLock<Option<RateEntry>>,
}

impl DataStore {
    pub fn new() -> Self {
        Self {
            signals: RwLock::new(VecDeque::with_capacity(MAX_SIGNALS)),
            tickers: RwLock::new(HashMap::new()),
            ticker_last_sample: RwLock::new(HashMap::new()),
            latest_tickers: RwLock::new(HashMap::new()),
            rates: RwLock::new(VecDeque::with_capacity(MAX_RATES)),
            latest_rate: RwLock::new(None),
        }
    }

    // -- Signals --

    pub fn push_signal(&self, signal: &ArbitrageSignal) {
        let stored = StoredSignal::from(signal);
        let mut buf = self.signals.write();
        if buf.len() >= MAX_SIGNALS {
            buf.pop_front();
        }
        buf.push_back(stored);
    }

    // -- Tickers --

    pub fn push_ticker(&self, t: &NormalizedTicker) {
        let key = (t.exchange, t.symbol);
        let snap = TickerSnapshot {
            bid_usd: t.best_bid_usd.to_f64().unwrap_or(0.0),
            ask_usd: t.best_ask_usd.to_f64().unwrap_or(0.0),
            raw_bid: t.raw_bid.to_f64().unwrap_or(0.0),
            raw_ask: t.raw_ask.to_f64().unwrap_or(0.0),
            timestamp: t.timestamp,
        };

        // Always update latest
        self.latest_tickers.write().insert(key, snap.clone());

        // Downsample: only store if enough time has passed
        let should_store = {
            let last = self.ticker_last_sample.read();
            match last.get(&key) {
                Some(ts) => {
                    (t.timestamp - *ts).num_milliseconds() >= TICKER_SAMPLE_INTERVAL_MS
                }
                None => true,
            }
        };

        if should_store {
            self.ticker_last_sample.write().insert(key, t.timestamp);
            let mut tickers = self.tickers.write();
            let buf = tickers
                .entry(key)
                .or_insert_with(|| VecDeque::with_capacity(MAX_TICKER_PER_PAIR));
            if buf.len() >= MAX_TICKER_PER_PAIR {
                buf.pop_front();
            }
            buf.push_back(snap);
        }
    }

    // -- Rates --

    pub fn push_rate(&self, rate: &ExchangeRate) {
        let entry = RateEntry {
            krw_per_usd: rate.krw_per_usd.to_f64().unwrap_or(0.0),
            source: format!("{:?}", rate.source),
            timestamp: rate.timestamp,
        };
        *self.latest_rate.write() = Some(entry.clone());
        let mut buf = self.rates.write();
        if buf.len() >= MAX_RATES {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    // -- Queries --

    pub fn snapshot(&self, latency: &[LatencySnapshot]) -> StateSnapshot {
        let latest = self.latest_tickers.read();
        let mut ticker_map = HashMap::new();
        for ((ex, sym), snap) in latest.iter() {
            ticker_map.insert(format!("{}:{}", ex, sym), snap.clone());
        }

        let recent_signals: Vec<StoredSignal> = {
            let buf = self.signals.read();
            buf.iter().rev().take(100).cloned().collect()
        };

        StateSnapshot {
            tickers: ticker_map,
            rate: self.latest_rate.read().clone(),
            latency: latency.to_vec(),
            recent_signals,
        }
    }

    pub fn memory_usage(&self) -> MemoryUsage {
        let signals_count = self.signals.read().len();
        let tickers = self.tickers.read();
        let ticker_pairs = tickers.len();
        let ticker_total: usize = tickers.values().map(|v| v.len()).sum();
        let rates_count = self.rates.read().len();

        let estimated_bytes =
            signals_count * 160 + ticker_total * 80 + rates_count * 32 + 1024;

        MemoryUsage {
            signals_count,
            signals_cap: MAX_SIGNALS,
            ticker_pairs,
            ticker_total_entries: ticker_total,
            ticker_cap_per_pair: MAX_TICKER_PER_PAIR,
            rates_count,
            rates_cap: MAX_RATES,
            estimated_mb: estimated_bytes as f64 / (1024.0 * 1024.0),
        }
    }

    pub fn query_performance(&self, period: Period) -> PerformanceStats {
        let buf = self.signals.read();
        let cutoff = period.cutoff();
        let trade_amount = 10_000.0_f64;

        let filtered: Vec<&StoredSignal> = buf.iter().filter(|s| s.timestamp >= cutoff).collect();

        if filtered.is_empty() {
            return PerformanceStats::empty();
        }

        let total_count = filtered.len();
        let total_profit: f64 = filtered.iter().map(|s| s.estimated_profit_usd).sum();
        let avg_spread: f64 =
            filtered.iter().map(|s| s.net_spread_pct).sum::<f64>() / total_count as f64;

        let period_hours = period.hours(&filtered);
        let annualized = if period_hours > 0.0 {
            (total_profit / trade_amount) * (365.0 * 24.0 / period_hours) * 100.0
        } else {
            0.0
        };

        // Hourly frequency
        let mut hourly: std::collections::BTreeMap<String, HourlyBucket> =
            std::collections::BTreeMap::new();
        for s in &filtered {
            let key = s.timestamp.format("%m/%d, %H").to_string();
            let entry = hourly.entry(key.clone()).or_insert(HourlyBucket {
                hour: key,
                count: 0,
                profit: 0.0,
            });
            entry.count += 1;
            entry.profit += s.estimated_profit_usd;
        }

        // Cumulative profit (max 200 points)
        let step = (filtered.len() / 200).max(1);
        let mut cumulative = Vec::new();
        let mut cum = 0.0;
        for (i, s) in filtered.iter().enumerate() {
            cum += s.estimated_profit_usd;
            if i % step == 0 || i == filtered.len() - 1 {
                cumulative.push(CumulativePoint {
                    timestamp: s.timestamp,
                    profit: cum,
                });
            }
        }

        // By symbol
        let mut by_sym: HashMap<String, SymbolStats> = HashMap::new();
        for s in &filtered {
            let e = by_sym.entry(s.symbol.clone()).or_insert(SymbolStats {
                symbol: s.symbol.clone(),
                count: 0,
                profit: 0.0,
                avg_spread: 0.0,
            });
            e.count += 1;
            e.profit += s.estimated_profit_usd;
            e.avg_spread += s.net_spread_pct;
        }
        let by_symbol: Vec<SymbolStats> = by_sym
            .into_values()
            .map(|mut v| {
                if v.count > 0 {
                    v.avg_spread /= v.count as f64;
                }
                v
            })
            .collect();

        // By pair
        let mut by_p: HashMap<String, PairStats> = HashMap::new();
        for s in &filtered {
            let key = format!("{} → {}", s.buy_exchange, s.sell_exchange);
            let e = by_p.entry(key.clone()).or_insert(PairStats {
                pair: key,
                count: 0,
                profit: 0.0,
            });
            e.count += 1;
            e.profit += s.estimated_profit_usd;
        }

        PerformanceStats {
            total_signals: total_count,
            total_profit,
            avg_spread,
            annualized_return: annualized,
            hourly_frequency: hourly.into_values().collect(),
            cumulative_profit: cumulative,
            by_symbol,
            by_pair: by_p.into_values().collect(),
        }
    }
}

// ============================================================
// Period / Stats types
// ============================================================

#[derive(Debug, Clone, Copy)]
pub enum Period {
    Hour1,
    Hour24,
    Day7,
    Day30,
    All,
}

impl Period {
    pub fn from_str(s: &str) -> Self {
        match s {
            "1h" => Period::Hour1,
            "24h" => Period::Hour24,
            "7d" => Period::Day7,
            "30d" => Period::Day30,
            _ => Period::All,
        }
    }

    fn cutoff(&self) -> DateTime<Utc> {
        match self {
            Period::Hour1 => Utc::now() - Duration::hours(1),
            Period::Hour24 => Utc::now() - Duration::hours(24),
            Period::Day7 => Utc::now() - Duration::days(7),
            Period::Day30 => Utc::now() - Duration::days(30),
            Period::All => DateTime::<Utc>::MIN_UTC,
        }
    }

    fn hours(&self, data: &[&StoredSignal]) -> f64 {
        match self {
            Period::Hour1 => 1.0,
            Period::Hour24 => 24.0,
            Period::Day7 => 168.0,
            Period::Day30 => 720.0,
            Period::All => {
                if data.len() < 2 {
                    return 1.0;
                }
                let first = data.first().unwrap().timestamp;
                let last = data.last().unwrap().timestamp;
                let h = (last - first).num_seconds() as f64 / 3600.0;
                h.max(1.0)
            }
        }
    }
}

#[derive(Serialize)]
pub struct PerformanceStats {
    pub total_signals: usize,
    pub total_profit: f64,
    pub avg_spread: f64,
    pub annualized_return: f64,
    pub hourly_frequency: Vec<HourlyBucket>,
    pub cumulative_profit: Vec<CumulativePoint>,
    pub by_symbol: Vec<SymbolStats>,
    pub by_pair: Vec<PairStats>,
}

impl PerformanceStats {
    fn empty() -> Self {
        Self {
            total_signals: 0,
            total_profit: 0.0,
            avg_spread: 0.0,
            annualized_return: 0.0,
            hourly_frequency: vec![],
            cumulative_profit: vec![],
            by_symbol: vec![],
            by_pair: vec![],
        }
    }
}

#[derive(Serialize, Clone)]
pub struct HourlyBucket {
    pub hour: String,
    pub count: usize,
    pub profit: f64,
}

#[derive(Serialize, Clone)]
pub struct CumulativePoint {
    pub timestamp: DateTime<Utc>,
    pub profit: f64,
}

#[derive(Serialize, Clone)]
pub struct SymbolStats {
    pub symbol: String,
    pub count: usize,
    pub profit: f64,
    pub avg_spread: f64,
}

#[derive(Serialize, Clone)]
pub struct PairStats {
    pub pair: String,
    pub count: usize,
    pub profit: f64,
}
