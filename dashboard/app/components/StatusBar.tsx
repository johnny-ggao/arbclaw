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
      className="border-b"
      style={{ background: "var(--bg-secondary)", borderColor: "var(--border)" }}
    >
      {/* Top row: logo + nav + status */}
      <div className="flex items-center justify-between px-3 sm:px-5 h-11">
        <div className="flex items-center gap-2 sm:gap-5 min-w-0">
          <span className="text-sm font-bold tracking-wide shrink-0" style={{ color: "var(--text-primary)" }}>
            CEX Arb
          </span>
          <nav className="flex items-center gap-1 text-xs">
            <span className="px-2 sm:px-3 py-1 rounded" style={{ color: "var(--text-primary)", background: "var(--bg-card-hover)" }}>
              监控
            </span>
            <Link href="/performance" className="px-2 sm:px-3 py-1 rounded transition-colors" style={{ color: "var(--text-muted)" }}>
              绩效
            </Link>
            <Link href="/orderbook" className="px-2 sm:px-3 py-1 rounded transition-colors" style={{ color: "var(--text-muted)" }}>
              盘口
            </Link>
          </nav>
          <div
            className="flex items-center gap-1 px-1.5 py-0.5 rounded text-xs shrink-0"
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
            <span className="hidden sm:inline">{connected ? "Live" : "Disconnected"}</span>
          </div>
        </div>
        <span className="text-xs shrink-0" style={{ color: "var(--text-muted)" }}>
          {tickCount.toLocaleString()} ticks
        </span>
      </div>
      {/* Bottom row: rates (collapses to compact on mobile) */}
      {rate && (
        <div
          className="flex items-center justify-center gap-3 sm:gap-5 px-3 pb-1.5 text-xs flex-wrap"
          style={{ color: "var(--text-muted)" }}
        >
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
        </div>
      )}
    </header>
  );
}
