use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

use crate::models::*;

const DEFAULT_TRADE_AMOUNT_USD: Decimal = dec!(10000);
const MIN_NET_SPREAD_PCT: Decimal = dec!(0.1); // 0.1%

pub struct ArbitrageEngine {
    latest: RwLock<HashMap<(Exchange, Symbol), NormalizedTicker>>,
    trade_amount_usd: Decimal,
    min_spread: Decimal,
}

impl ArbitrageEngine {
    pub fn new() -> Self {
        Self {
            latest: RwLock::new(HashMap::new()),
            trade_amount_usd: DEFAULT_TRADE_AMOUNT_USD,
            min_spread: MIN_NET_SPREAD_PCT,
        }
    }

    pub fn update(&self, ticker: NormalizedTicker) -> Vec<ArbitrageSignal> {
        let symbol = ticker.symbol;
        self.latest
            .write()
            .insert((ticker.exchange, ticker.symbol), ticker);

        self.scan_opportunities(symbol)
    }

    fn scan_opportunities(&self, symbol: Symbol) -> Vec<ArbitrageSignal> {
        let tickers = self.latest.read();
        let exchanges = [
            Exchange::Binance,
            Exchange::Bybit,
            Exchange::Upbit,
            Exchange::Bithumb,
        ];

        let mut signals = Vec::new();
        let now = Utc::now();
        let hundred = dec!(100);

        for &buy_ex in &exchanges {
            for &sell_ex in &exchanges {
                if buy_ex == sell_ex {
                    continue;
                }

                let buy_ticker = match tickers.get(&(buy_ex, symbol)) {
                    Some(t) => t,
                    None => continue,
                };
                let sell_ticker = match tickers.get(&(sell_ex, symbol)) {
                    Some(t) => t,
                    None => continue,
                };

                // Check staleness (3 seconds)
                let buy_age = (now - buy_ticker.local_timestamp).num_seconds();
                let sell_age = (now - sell_ticker.local_timestamp).num_seconds();
                if buy_age > 3 || sell_age > 3 {
                    continue;
                }

                let buy_ask = buy_ticker.best_ask_usd;
                let sell_bid = sell_ticker.best_bid_usd;

                if buy_ask.is_zero() {
                    continue;
                }

                let gross_spread = (sell_bid - buy_ask) / buy_ask * hundred;
                let total_fee = (buy_ex.taker_fee() + sell_ex.taker_fee()) * hundred;
                let net_spread = gross_spread - total_fee;

                if net_spread < self.min_spread {
                    continue;
                }

                // Calculate max quantity and profit
                let max_qty_by_amount = self.trade_amount_usd / buy_ask;
                let max_qty = max_qty_by_amount
                    .min(buy_ticker.best_ask_qty)
                    .min(sell_ticker.best_bid_qty);

                let buy_cost = max_qty * buy_ask * (Decimal::ONE + buy_ex.taker_fee());
                let sell_revenue = max_qty * sell_bid * (Decimal::ONE - sell_ex.taker_fee());
                let profit = sell_revenue - buy_cost;

                signals.push(ArbitrageSignal {
                    buy_exchange: buy_ex,
                    sell_exchange: sell_ex,
                    symbol,
                    gross_spread_pct: gross_spread,
                    net_spread_pct: net_spread,
                    max_qty,
                    estimated_profit_usd: profit,
                    buy_price_usd: buy_ask,
                    sell_price_usd: sell_bid,
                    timestamp: now,
                });
            }
        }

        signals
    }
}
