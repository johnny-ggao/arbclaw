"use client";
import { useEffect, useRef, useCallback, useState } from "react";
import type {
  Exchange,
  Symbol,
  TickerState,
  SignalRecord,
  WsMessage,
  NormalizedTicker,
  ArbitrageSignal,
  ExchangeRate,
  ExchangeLatencyInfo,
  LatencyReport,
} from "@/app/lib/types";

function getWsUrl(): string {
  if (typeof window === "undefined") return "ws://localhost/ws";
  const p = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${p}//${window.location.host}/ws`;
}

function getApiBase(): string {
  if (typeof window === "undefined") return "http://localhost";
  return `${window.location.protocol}//${window.location.host}`;
}

const MAX_SIGNALS = 200;

export interface DashboardState {
  tickers: Record<string, TickerState>;
  signals: SignalRecord[];
  rate: { krw_per_usd: number; source: string } | null;
  latency: ExchangeLatencyInfo[];
  connected: boolean;
  tickCount: number;
}

function tickerKey(exchange: Exchange, symbol: Symbol) {
  return `${exchange}:${symbol}`;
}

export function useWebSocket(): DashboardState {
  const [state, setState] = useState<DashboardState>({
    tickers: {},
    signals: [],
    rate: null,
    latency: [],
    connected: false,
    tickCount: 0,
  });

  const wsRef = useRef<WebSocket | null>(null);
  const signalIdRef = useRef(0);
  const stateRef = useRef(state);
  stateRef.current = state;

  const pendingRef = useRef<Record<string, TickerState>>({});
  const pendingSignalsRef = useRef<SignalRecord[]>([]);
  const pendingRateRef = useRef<{ krw_per_usd: number; source: string } | null>(null);
  const pendingLatencyRef = useRef<ExchangeLatencyInfo[] | null>(null);
  const tickAccRef = useRef(0);

  // Fetch initial snapshot from backend on mount
  useEffect(() => {
    fetch(`${getApiBase()}/api/snapshot`)
      .then((r) => r.json())
      .then((snap) => {
        const tickers: Record<string, TickerState> = {};
        if (snap.tickers) {
          for (const [key, t] of Object.entries(snap.tickers) as [string, any][]) {
            tickers[key] = {
              best_bid_usd: t.bid_usd,
              best_ask_usd: t.ask_usd,
              best_bid_qty: 0,
              best_ask_qty: 0,
              raw_bid: t.raw_bid,
              raw_ask: t.raw_ask,
              quote_currency: t.raw_bid !== t.bid_usd ? "KRW" : "USDT",
              exchange_rate: null,
              updatedAt: new Date(t.timestamp).getTime(),
            };
          }
        }

        const signals: SignalRecord[] = (snap.recent_signals || []).map(
          (s: any, i: number) => ({
            type: "signal" as const,
            buy_exchange: s.buy_exchange,
            sell_exchange: s.sell_exchange,
            symbol: s.symbol,
            net_spread_pct: String(s.net_spread_pct),
            gross_spread_pct: String(s.gross_spread_pct),
            estimated_profit_usd: String(s.estimated_profit_usd),
            max_qty: String(s.max_qty),
            buy_price_usd: String(s.buy_price_usd),
            sell_price_usd: String(s.sell_price_usd),
            timestamp: s.timestamp,
            id: i + 1,
            receivedAt: new Date(s.timestamp).getTime(),
          })
        );
        signalIdRef.current = signals.length;

        const rate = snap.rate
          ? { krw_per_usd: snap.rate.krw_per_usd, source: snap.rate.source }
          : null;

        const latency: ExchangeLatencyInfo[] = snap.latency || [];

        setState((prev) => ({
          ...prev,
          tickers,
          signals,
          rate,
          latency,
        }));
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const flushInterval = setInterval(() => {
      const pending = pendingRef.current;
      const pendingSignals = pendingSignalsRef.current;
      const pendingRate = pendingRateRef.current;
      const pendingLatency = pendingLatencyRef.current;
      const ticks = tickAccRef.current;

      if (
        Object.keys(pending).length === 0 &&
        pendingSignals.length === 0 &&
        !pendingRate &&
        !pendingLatency &&
        ticks === 0
      )
        return;

      pendingRef.current = {};
      pendingSignalsRef.current = [];
      pendingRateRef.current = null;
      pendingLatencyRef.current = null;
      tickAccRef.current = 0;

      setState((prev) => {
        const newTickers = { ...prev.tickers, ...pending };
        const newSignals =
          pendingSignals.length > 0
            ? [...pendingSignals, ...prev.signals].slice(0, MAX_SIGNALS)
            : prev.signals;
        return {
          ...prev,
          tickers: newTickers,
          signals: newSignals,
          rate: pendingRate || prev.rate,
          latency: pendingLatency || prev.latency,
          tickCount: prev.tickCount + ticks,
        };
      });
    }, 250);

    return () => clearInterval(flushInterval);
  }, []);

  const handleMessage = useCallback((data: string) => {
    try {
      const msg: WsMessage = JSON.parse(data);
      switch (msg.type) {
        case "ticker": {
          const t = msg as NormalizedTicker;
          const key = tickerKey(t.exchange, t.symbol);
          pendingRef.current[key] = {
            best_bid_usd: parseFloat(t.best_bid_usd),
            best_ask_usd: parseFloat(t.best_ask_usd),
            best_bid_qty: parseFloat(t.best_bid_qty),
            best_ask_qty: parseFloat(t.best_ask_qty),
            raw_bid: parseFloat(t.raw_bid),
            raw_ask: parseFloat(t.raw_ask),
            quote_currency: t.quote_currency,
            exchange_rate: t.exchange_rate ? parseFloat(t.exchange_rate) : null,
            updatedAt: Date.now(),
          };
          tickAccRef.current++;
          break;
        }
        case "signal": {
          const s = msg as ArbitrageSignal;
          signalIdRef.current++;
          pendingSignalsRef.current.push({
            ...s,
            id: signalIdRef.current,
            receivedAt: Date.now(),
          });
          break;
        }
        case "rate": {
          const r = msg as ExchangeRate;
          pendingRateRef.current = {
            krw_per_usd: parseFloat(r.krw_per_usd),
            source: r.source,
          };
          break;
        }
        case "latency": {
          const l = msg as LatencyReport;
          pendingLatencyRef.current = l.exchanges;
          break;
        }
      }
    } catch {}
  }, []);

  useEffect(() => {
    let reconnectTimer: ReturnType<typeof setTimeout>;
    let ws: WebSocket;

    function connect() {
      ws = new WebSocket(getWsUrl());
      wsRef.current = ws;

      ws.onopen = () => {
        setState((p) => ({ ...p, connected: true }));
      };

      ws.onmessage = (event) => {
        handleMessage(event.data);
      };

      ws.onclose = () => {
        setState((p) => ({ ...p, connected: false }));
        reconnectTimer = setTimeout(connect, 2000);
      };

      ws.onerror = () => {
        ws.close();
      };
    }

    connect();

    return () => {
      clearTimeout(reconnectTimer);
      if (wsRef.current) wsRef.current.close();
    };
  }, [handleMessage]);

  return state;
}
