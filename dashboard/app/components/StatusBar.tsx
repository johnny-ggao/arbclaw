"use client";
import Link from "next/link";

interface RateInfo {
  krw_per_usdt: number;
  usdt_per_usd: number;
  krw_per_usd: number;
  source: string;
}

interface Props {
  connected: boolean;
  tickCount: number;
  rate: RateInfo | null;
}

export default function StatusBar({ connected, tickCount, rate }: Props) {
  return (
    <header
      className="flex items-center justify-between px-5 h-12 border-b"
      style={{ background: "var(--bg-secondary)", borderColor: "var(--border)" }}
    >
      <div className="flex items-center gap-5">
        <span className="text-sm font-bold tracking-wide" style={{ color: "var(--text-primary)" }}>
          CEX Arbitrage
        </span>
        <nav className="flex items-center gap-1 text-xs">
          <span className="px-3 py-1.5 rounded" style={{ color: "var(--text-primary)", background: "var(--bg-card-hover)" }}>
            实时监控
          </span>
          <Link href="/performance" className="px-3 py-1.5 rounded transition-colors" style={{ color: "var(--text-muted)" }}>
            绩效分析
          </Link>
        </nav>
        <div
          className="flex items-center gap-1.5 px-2 py-0.5 rounded text-xs"
          style={{
            background: connected ? "var(--green-bg)" : "var(--red-bg)",
            color: connected ? "var(--green)" : "var(--red)",
          }}
        >
          <span
            className="w-1.5 h-1.5 rounded-full"
            style={{
              background: connected ? "var(--green)" : "var(--red)",
              animation: connected ? "pulse-dot 2s infinite" : "none",
            }}
          />
          {connected ? "Live" : "Disconnected"}
        </div>
      </div>
      <div className="flex items-center gap-5 text-xs" style={{ color: "var(--text-muted)" }}>
        <span>{tickCount.toLocaleString()} ticks</span>
        {rate && (
          <>
            <span>
              KRW/USDT{" "}
              <span style={{ color: "var(--text-secondary)" }}>{rate.krw_per_usdt.toFixed(2)}</span>
            </span>
            <span>
              USDT/USD{" "}
              <span style={{ color: rate.usdt_per_usd >= 1 ? "var(--green)" : "var(--red)" }}>
                {rate.usdt_per_usd.toFixed(4)}
              </span>
            </span>
            <span>
              KRW/USD{" "}
              <span style={{ color: "var(--text-secondary)" }}>{rate.krw_per_usd.toFixed(2)}</span>
            </span>
          </>
        )}
      </div>
    </header>
  );
}
