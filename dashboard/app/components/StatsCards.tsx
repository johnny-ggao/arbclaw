"use client";
import { useMemo } from "react";
import { EXCHANGES, SYMBOLS, EXCHANGE_META, type TickerState } from "@/app/lib/types";

interface Props {
  tickers: Record<string, TickerState>;
  signalCount: number;
  rate: { krw_per_usdt: number; usdt_per_usd: number; krw_per_usd: number; source: string } | null;
}

export default function StatsCards({ tickers, signalCount, rate }: Props) {
  const stats = useMemo(() => {
    let bestSpread = -Infinity;
    let bestLabel = "";
    for (const sym of SYMBOLS) {
      for (const bx of EXCHANGES) {
        for (const sx of EXCHANGES) {
          if (bx === sx) continue;
          const b = tickers[`${bx}:${sym}`];
          const s = tickers[`${sx}:${sym}`];
          if (!b || !s || b.best_ask_usd === 0) continue;
          const net = ((s.best_bid_usd - b.best_ask_usd) / b.best_ask_usd) * 100
            - (EXCHANGE_META[bx].fee + EXCHANGE_META[sx].fee) * 100;
          if (net > bestSpread) { bestSpread = net; bestLabel = `${sym} ${bx}→${sx}`; }
        }
      }
    }
    const active = Object.values(tickers).filter(t => Date.now() - t.updatedAt < 5000).length;
    return { bestSpread, bestLabel, active };
  }, [tickers]);

  const items = [
    {
      label: "Best Spread",
      value: stats.bestSpread > -Infinity ? `${stats.bestSpread >= 0 ? "+" : ""}${stats.bestSpread.toFixed(3)}%` : "—",
      sub: stats.bestLabel,
      color: stats.bestSpread > 0 ? "var(--green)" : stats.bestSpread > -0.3 ? "var(--text-secondary)" : "var(--red)",
    },
    {
      label: "Active Feeds",
      value: `${stats.active}/${EXCHANGES.length * SYMBOLS.length}`,
      color: stats.active >= 12 ? "var(--green)" : "var(--yellow)",
    },
    {
      label: "Signals",
      value: String(signalCount),
      color: signalCount > 0 ? "var(--green)" : "var(--text-muted)",
    },
    {
      label: "KRW/USDT",
      value: rate ? rate.krw_per_usdt.toFixed(2) : "—",
      sub: rate ? `USDT/USD ${rate.usdt_per_usd.toFixed(4)} · KRW/USD ${rate.krw_per_usd.toFixed(2)}` : undefined,
      color: "var(--text-primary)",
    },
  ];

  return (
    <div className="grid grid-cols-2 lg:grid-cols-4 gap-px" style={{ background: "var(--border)" }}>
      {items.map((c) => (
        <div key={c.label} className="px-4 py-3" style={{ background: "var(--bg-secondary)" }}>
          <div className="text-xs mb-1.5" style={{ color: "var(--text-muted)" }}>{c.label}</div>
          <div className="text-lg font-semibold leading-none" style={{ color: c.color }}>{c.value}</div>
          {c.sub && <div className="text-xs mt-1 truncate" style={{ color: "var(--text-muted)" }}>{c.sub}</div>}
        </div>
      ))}
    </div>
  );
}
