export type Exchange = "Binance" | "Bybit" | "Upbit" | "Bithumb";
export type Symbol = "BTC" | "ETH" | "SOL" | "XRP";

export const EXCHANGES: Exchange[] = ["Binance", "Bybit", "Upbit", "Bithumb"];
export const SYMBOLS: Symbol[] = ["BTC", "ETH", "SOL", "XRP"];

export const EXCHANGE_META: Record<Exchange, { quote: "USDT" | "KRW"; fee: number; color: string }> = {
  Binance: { quote: "USDT", fee: 0.001, color: "#f0b90b" },
  Bybit:   { quote: "USDT", fee: 0.001, color: "#f7a600" },
  Upbit:   { quote: "KRW",  fee: 0.0025, color: "#093687" },
  Bithumb: { quote: "KRW",  fee: 0.0025, color: "#f26522" },
};

export interface NormalizedTicker {
  type: "ticker";
  exchange: Exchange;
  symbol: Symbol;
  best_bid_usd: string;
  best_bid_qty: string;
  best_ask_usd: string;
  best_ask_qty: string;
  raw_bid: string;
  raw_ask: string;
  quote_currency: "USDT" | "KRW";
  exchange_rate: string | null;
  timestamp: string;
  local_timestamp: string;
}

export interface ArbitrageSignal {
  type: "signal";
  buy_exchange: Exchange;
  sell_exchange: Exchange;
  symbol: Symbol;
  gross_spread_pct: string;
  net_spread_pct: string;
  max_qty: string;
  estimated_profit_usd: string;
  buy_price_usd: string;
  sell_price_usd: string;
  timestamp: string;
}

export interface ExchangeRate {
  type: "rate";
  krw_per_usdt: string;
  usdt_per_usd: string;
  krw_per_usd: string;
  source: "Implied" | "External";
  timestamp: string;
}

export interface FeedStatus {
  type: "status";
  exchange: Exchange;
  connected: boolean;
  last_update: string | null;
  stale: boolean;
}

export interface LatencyReport {
  type: "latency";
  exchanges: ExchangeLatencyInfo[];
}

export interface ExchangeLatencyInfo {
  exchange: string;
  last_rtt_ms: number;
  avg_rtt_ms: number;
  min_rtt_ms: number;
  max_rtt_ms: number;
  samples: number;
}

export interface PriceLevel {
  price: string;
  qty: string;
}

export interface OrderBookUpdate {
  type: "orderbook";
  exchange: Exchange;
  symbol: Symbol;
  bids: PriceLevel[];
  asks: PriceLevel[];
  quote_currency: "USDT" | "KRW";
  timestamp: string;
}

export type WsMessage = NormalizedTicker | ArbitrageSignal | ExchangeRate | FeedStatus | LatencyReport | OrderBookUpdate;

export interface TickerState {
  best_bid_usd: number;
  best_ask_usd: number;
  best_bid_qty: number;
  best_ask_qty: number;
  raw_bid: number;
  raw_ask: number;
  quote_currency: "USDT" | "KRW";
  exchange_rate: number | null;
  updatedAt: number;
}

export interface SignalRecord extends ArbitrageSignal {
  id: number;
  receivedAt: number;
}
