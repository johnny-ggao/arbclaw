"use client";
import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import { fetchPerformance, type PerformanceStats, type PeriodKey } from "@/app/lib/api";
import FrequencyChart from "./FrequencyChart";
import CumulativeChart from "./CumulativeChart";
import SymbolChart from "./SymbolChart";
import PairChart from "./PairChart";

const PERIODS: { key: PeriodKey; label: string }[] = [
  { key: "1h", label: "1小时" },
  { key: "24h", label: "24小时" },
  { key: "7d", label: "7天" },
  { key: "30d", label: "30天" },
  { key: "all", label: "全部" },
];

function fmtNum(n: number, d: number = 2): string {
  return n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
}

export default function PerformancePage() {
  const [period, setPeriod] = useState<PeriodKey>("all");
  const [data, setData] = useState<PerformanceStats | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const stats = await fetchPerformance(period);
      setData(stats);
    } catch {
      // API not available
    } finally {
      setLoading(false);
    }
  }, [period]);

  useEffect(() => {
    setLoading(true);
    load();
    const iv = setInterval(load, 5000);
    return () => clearInterval(iv);
  }, [load]);

  const cards = data
    ? [
        {
          label: "预估年化收益率",
          value: `${fmtNum(data.annualized_return, 1)}%`,
          color: data.annualized_return >= 0 ? "var(--green)" : "var(--red)",
        },
        {
          label: "总机会数",
          value: data.total_signals.toLocaleString(),
          color: "var(--blue)",
        },
        {
          label: "平均溢价",
          value: `${fmtNum(data.avg_spread, 2)}%`,
          color: data.avg_spread >= 0 ? "var(--green)" : "var(--red)",
        },
        {
          label: "预估利润 (USD)",
          value: `$${fmtNum(data.total_profit)}`,
          color: data.total_profit >= 0 ? "var(--green)" : "var(--red)",
        },
      ]
    : [];

  return (
    <div className="min-h-screen flex flex-col" style={{ background: "var(--bg-primary)" }}>
      {/* Header */}
      <header
        className="flex items-center justify-between px-3 sm:px-5 h-11 border-b"
        style={{ background: "var(--bg-secondary)", borderColor: "var(--border)" }}
      >
        <div className="flex items-center gap-2 sm:gap-6">
          <span className="text-sm font-bold" style={{ color: "var(--text-primary)" }}>
            CEX Arb
          </span>
          <nav className="flex items-center gap-1 text-xs">
            <Link href="/" className="px-2 sm:px-3 py-1 rounded transition-colors" style={{ color: "var(--text-muted)" }}>
              监控
            </Link>
            <span className="px-2 sm:px-3 py-1 rounded" style={{ color: "var(--text-primary)", background: "var(--bg-card-hover)" }}>
              绩效
            </span>
            <Link href="/orderbook" className="px-2 sm:px-3 py-1 rounded transition-colors" style={{ color: "var(--text-muted)" }}>
              盘口
            </Link>
          </nav>
        </div>
      </header>

      <main className="flex-1 p-3 sm:p-4 lg:p-6 max-w-[1400px] mx-auto w-full">
        {/* Title + Period */}
        <div className="flex items-center justify-between mb-4 gap-2">
          <h1 className="text-base sm:text-lg font-bold shrink-0" style={{ color: "var(--text-primary)" }}>
            绩效仪表板
          </h1>
          <div className="flex rounded overflow-hidden border shrink-0" style={{ borderColor: "var(--border)" }}>
            {PERIODS.map((p) => (
              <button
                key={p.key}
                onClick={() => setPeriod(p.key)}
                className="px-2 sm:px-3 py-1 text-xs cursor-pointer transition-colors"
                style={{
                  background: period === p.key ? "var(--bg-card-hover)" : "transparent",
                  color: period === p.key ? "var(--text-primary)" : "var(--text-muted)",
                  borderRight: "1px solid var(--border)",
                }}
              >
                {p.label}
              </button>
            ))}
          </div>
        </div>

        {loading && !data ? (
          <div className="flex items-center justify-center py-20 text-sm" style={{ color: "var(--text-muted)" }}>
            Loading...
          </div>
        ) : !data ? (
          <div className="flex items-center justify-center py-20 text-sm" style={{ color: "var(--text-muted)" }}>
            无法连接到引擎 API
          </div>
        ) : (
          <>
            {/* Stat Cards */}
            <div className="grid grid-cols-2 lg:grid-cols-4 gap-2 sm:gap-3 mb-4">
              {cards.map((c) => (
                <div
                  key={c.label}
                  className="rounded-lg border px-3 sm:px-5 py-3 sm:py-4"
                  style={{ borderColor: "var(--border)", background: "var(--bg-card)" }}
                >
                  <div className="text-xs mb-1.5" style={{ color: "var(--text-muted)" }}>
                    {c.label}
                  </div>
                  <div className="text-base sm:text-xl font-bold font-mono" style={{ color: c.color }}>
                    {c.value}
                  </div>
                </div>
              ))}
            </div>

            {/* Charts Row 1 */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-2 sm:gap-3 mb-2 sm:mb-3">
              <div
                className="rounded-lg border p-4"
                style={{ borderColor: "var(--border)", background: "var(--bg-card)" }}
              >
                <h3 className="text-xs font-medium mb-3" style={{ color: "var(--text-secondary)" }}>
                  套利机会频率
                </h3>
                <FrequencyChart data={data.hourly_frequency} />
              </div>
              <div
                className="rounded-lg border p-4"
                style={{ borderColor: "var(--border)", background: "var(--bg-card)" }}
              >
                <h3 className="text-xs font-medium mb-3" style={{ color: "var(--text-secondary)" }}>
                  累计预估利润 (USD)
                </h3>
                <CumulativeChart data={data.cumulative_profit} />
              </div>
            </div>

            {/* Charts Row 2 */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-2 sm:gap-3">
              <div
                className="rounded-lg border p-4"
                style={{ borderColor: "var(--border)", background: "var(--bg-card)" }}
              >
                <h3 className="text-xs font-medium mb-3" style={{ color: "var(--text-secondary)" }}>
                  按币种分析
                </h3>
                <SymbolChart data={data.by_symbol} />
              </div>
              <div
                className="rounded-lg border p-4"
                style={{ borderColor: "var(--border)", background: "var(--bg-card)" }}
              >
                <h3 className="text-xs font-medium mb-3" style={{ color: "var(--text-secondary)" }}>
                  按交易所配对分析
                </h3>
                <PairChart data={data.by_pair} />
              </div>
            </div>
          </>
        )}
      </main>

      <footer
        className="px-5 py-2 text-center text-xs border-t"
        style={{ borderColor: "var(--border)", color: "var(--text-muted)" }}
      >
        Simulation Mode — Not Financial Advice
      </footer>
    </div>
  );
}
