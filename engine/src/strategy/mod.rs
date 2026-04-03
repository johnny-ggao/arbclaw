use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

use crate::models::*;

const DEFAULT_TRADE_AMOUNT_USD: Decimal = dec!(10000);
const MIN_SPREAD_PCT: Decimal = dec!(2.0); // 2.0% gross (fees excluded from threshold)

pub struct ArbitrageEngine {
    latest: RwLock<HashMap<(Exchange, Symbol), NormalizedTicker>>,
    order_books: RwLock<HashMap<(Exchange, Symbol), OrderBookUpdate>>,
    trade_amount_usd: Decimal,
    min_spread: Decimal,
}

impl ArbitrageEngine {
    pub fn new() -> Self {
        Self {
            latest: RwLock::new(HashMap::new()),
            order_books: RwLock::new(HashMap::new()),
            trade_amount_usd: DEFAULT_TRADE_AMOUNT_USD,
            min_spread: MIN_SPREAD_PCT,
        }
    }

    /// Store latest order book snapshot (does not trigger scan).
    /// Defensively sorts levels to guarantee VWAP invariant.
    pub fn update_order_book(&self, mut ob: OrderBookUpdate) {
        ob.asks.sort_by(|a, b| a.price.cmp(&b.price)); // low→high
        ob.bids.sort_by(|a, b| b.price.cmp(&a.price)); // high→low
        self.order_books
            .write()
            .insert((ob.exchange, ob.symbol), ob);
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
        let books = self.order_books.read();
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

                // BBO spread (for display)
                let bbo_spread = (sell_bid - buy_ask) / buy_ask * hundred;

                // Try VWAP from order book depth
                let buy_book = books.get(&(buy_ex, symbol));
                let sell_book = books.get(&(sell_ex, symbol));

                // Compute VWAP from order book depth, or fallback to BBO
                let bbo_fallback = || {
                    let mq = (self.trade_amount_usd / buy_ask)
                        .min(buy_ticker.best_ask_qty)
                        .min(sell_ticker.best_bid_qty);
                    let p = mq * (sell_bid - buy_ask);
                    (buy_ask, sell_bid, bbo_spread, mq, p)
                };

                let (vwap_buy, vwap_sell, vwap_spread, max_qty, profit) =
                    if let (Some(bb), Some(sb)) = (buy_book, sell_book) {
                        if bb.asks.is_empty() || sb.bids.is_empty() {
                            bbo_fallback()
                        } else {
                            let buy_to_usd = if !buy_ticker.raw_ask.is_zero() {
                                buy_ticker.best_ask_usd / buy_ticker.raw_ask
                            } else {
                                Decimal::ONE
                            };
                            let sell_to_usd = if !sell_ticker.raw_bid.is_zero() {
                                sell_ticker.best_bid_usd / sell_ticker.raw_bid
                            } else {
                                Decimal::ONE
                            };

                            match vwap_take_asks(&bb.asks, self.trade_amount_usd, buy_to_usd) {
                                Some((vb_full, qty_bought)) => {
                                    match vwap_take_bids(&sb.bids, qty_bought, sell_to_usd) {
                                        Some((vs, qty_sold)) => {
                                            // If sell side can't absorb all, recalc buy VWAP
                                            // for the actual tradeable qty (shallower = better price)
                                            let vb = if qty_sold < qty_bought {
                                                vwap_take_asks_by_qty(
                                                    &bb.asks, qty_sold, buy_to_usd,
                                                )
                                                .map(|(v, _)| v)
                                                .unwrap_or(vb_full)
                                            } else {
                                                vb_full
                                            };
                                            let spread = if !vb.is_zero() {
                                                (vs - vb) / vb * hundred
                                            } else {
                                                bbo_spread
                                            };
                                            let p = qty_sold * (vs - vb);
                                            (vb, vs, spread, qty_sold, p)
                                        }
                                        None => bbo_fallback(),
                                    }
                                }
                                None => bbo_fallback(),
                            }
                        }
                    } else {
                        bbo_fallback()
                    };

                // Use VWAP spread for threshold judgment
                if vwap_spread < self.min_spread {
                    continue;
                }

                signals.push(ArbitrageSignal {
                    buy_exchange: buy_ex,
                    sell_exchange: sell_ex,
                    symbol,
                    gross_spread_pct: bbo_spread,
                    net_spread_pct: bbo_spread,
                    max_qty,
                    estimated_profit_usd: profit,
                    buy_price_usd: buy_ask,   // BBO for display
                    sell_price_usd: sell_bid,  // BBO for display
                    vwap_buy_usd: vwap_buy,
                    vwap_sell_usd: vwap_sell,
                    vwap_spread_pct: vwap_spread,
                    timestamp: now,
                });
            }
        }

        signals
    }
}

// ── VWAP helpers ────────────────────────────────────────────

/// Walk ask levels (sorted low→high), spending up to `budget_usd`.
/// Returns `(vwap_usd, filled_qty)`.
fn vwap_take_asks(
    asks: &[PriceLevel],
    budget_usd: Decimal,
    to_usd: Decimal,
) -> Option<(Decimal, Decimal)> {
    let mut filled_qty = Decimal::ZERO;
    let mut spent_usd = Decimal::ZERO;

    for level in asks {
        let price_usd = level.price * to_usd;
        if price_usd.is_zero() {
            continue;
        }
        let remaining = budget_usd - spent_usd;
        if remaining <= Decimal::ZERO {
            break;
        }
        let level_cost = level.qty * price_usd;
        if level_cost >= remaining {
            let partial_qty = remaining / price_usd;
            filled_qty += partial_qty;
            spent_usd = budget_usd;
        } else {
            filled_qty += level.qty;
            spent_usd += level_cost;
        }
    }

    if filled_qty.is_zero() {
        return None;
    }
    Some((spent_usd / filled_qty, filled_qty))
}

/// Walk ask levels (sorted low→high), buying up to `target_qty` coins.
/// Used to recalculate buy VWAP when sell depth limits the tradeable quantity.
/// Returns `(vwap_usd, filled_qty)`.
fn vwap_take_asks_by_qty(
    asks: &[PriceLevel],
    target_qty: Decimal,
    to_usd: Decimal,
) -> Option<(Decimal, Decimal)> {
    let mut filled_qty = Decimal::ZERO;
    let mut spent_usd = Decimal::ZERO;

    for level in asks {
        let price_usd = level.price * to_usd;
        if price_usd.is_zero() {
            continue;
        }
        let remaining = target_qty - filled_qty;
        if remaining <= Decimal::ZERO {
            break;
        }
        if level.qty >= remaining {
            filled_qty = target_qty;
            spent_usd += remaining * price_usd;
        } else {
            filled_qty += level.qty;
            spent_usd += level.qty * price_usd;
        }
    }

    if filled_qty.is_zero() {
        return None;
    }
    Some((spent_usd / filled_qty, filled_qty))
}

/// Walk bid levels (sorted high→low), selling up to `target_qty`.
/// Returns `(vwap_usd, filled_qty)`.
fn vwap_take_bids(
    bids: &[PriceLevel],
    target_qty: Decimal,
    to_usd: Decimal,
) -> Option<(Decimal, Decimal)> {
    let mut filled_qty = Decimal::ZERO;
    let mut proceeds_usd = Decimal::ZERO;

    for level in bids {
        let price_usd = level.price * to_usd;
        if price_usd.is_zero() {
            continue;
        }
        let remaining = target_qty - filled_qty;
        if remaining <= Decimal::ZERO {
            break;
        }
        if level.qty >= remaining {
            filled_qty = target_qty;
            proceeds_usd += remaining * price_usd;
        } else {
            filled_qty += level.qty;
            proceeds_usd += level.qty * price_usd;
        }
    }

    if filled_qty.is_zero() {
        return None;
    }
    Some((proceeds_usd / filled_qty, filled_qty))
}
