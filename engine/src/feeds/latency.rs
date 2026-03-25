use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;

use crate::models::Exchange;

const HISTORY_SIZE: usize = 60;

#[derive(Debug, Clone, Serialize)]
pub struct LatencySnapshot {
    pub exchange: String,
    pub last_rtt_ms: f64,
    pub avg_rtt_ms: f64,
    pub min_rtt_ms: f64,
    pub max_rtt_ms: f64,
    pub samples: usize,
    pub updated_at: DateTime<Utc>,
}

struct ExchangeLatency {
    history: Vec<f64>,
    updated_at: DateTime<Utc>,
}

impl ExchangeLatency {
    fn new() -> Self {
        Self {
            history: Vec::with_capacity(HISTORY_SIZE),
            updated_at: Utc::now(),
        }
    }

    fn push(&mut self, rtt_ms: f64) {
        if self.history.len() >= HISTORY_SIZE {
            self.history.remove(0);
        }
        self.history.push(rtt_ms);
        self.updated_at = Utc::now();
    }

    fn snapshot(&self, exchange: Exchange) -> LatencySnapshot {
        let len = self.history.len();
        if len == 0 {
            return LatencySnapshot {
                exchange: exchange.to_string(),
                last_rtt_ms: 0.0,
                avg_rtt_ms: 0.0,
                min_rtt_ms: 0.0,
                max_rtt_ms: 0.0,
                samples: 0,
                updated_at: self.updated_at,
            };
        }
        let last = self.history[len - 1];
        let sum: f64 = self.history.iter().sum();
        let min = self.history.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = self.history.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        LatencySnapshot {
            exchange: exchange.to_string(),
            last_rtt_ms: last,
            avg_rtt_ms: sum / len as f64,
            min_rtt_ms: min,
            max_rtt_ms: max,
            samples: len,
            updated_at: self.updated_at,
        }
    }
}

pub struct LatencyTracker {
    data: RwLock<HashMap<Exchange, ExchangeLatency>>,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    pub fn record(&self, exchange: Exchange, rtt_ms: f64) {
        let mut data = self.data.write();
        data.entry(exchange)
            .or_insert_with(ExchangeLatency::new)
            .push(rtt_ms);
    }

    pub fn snapshots(&self) -> Vec<LatencySnapshot> {
        let data = self.data.read();
        let exchanges = [Exchange::Binance, Exchange::Bybit, Exchange::Upbit, Exchange::Bithumb];
        exchanges
            .iter()
            .map(|ex| {
                data.get(ex)
                    .map(|l| l.snapshot(*ex))
                    .unwrap_or(LatencySnapshot {
                        exchange: ex.to_string(),
                        last_rtt_ms: 0.0,
                        avg_rtt_ms: 0.0,
                        min_rtt_ms: 0.0,
                        max_rtt_ms: 0.0,
                        samples: 0,
                        updated_at: Utc::now(),
                    })
            })
            .collect()
    }
}
