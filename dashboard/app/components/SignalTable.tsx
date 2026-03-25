"use client";
import type { SignalRecord } from "@/app/lib/types";

interface Props {
  signals: SignalRecord[];
}

function timeAgo(ts: number): string {
  const s = Math.floor((Date.now() - ts) / 1000);
  if (s < 5) return "now";
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  return `${Math.floor(s / 3600)}h`;
}

export default function SignalTable({ signals }: Props) {
  const profitable = signals.filter(s => parseFloat(s.net_spread_pct) > 0);
  const rows = profitable.length > 0 ? profitable.slice(0, 50) : signals.slice(0, 50);

  return (
    <div className="border rounded" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
      <div className="px-4 py-2.5 border-b flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
        <span className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>Arbitrage Signals</span>
        <span className="text-xs" style={{ color: profitable.length > 0 ? "var(--green)" : "var(--text-muted)" }}>
          {profitable.length > 0 ? `${profitable.length} profitable` : "no opportunities"}
        </span>
      </div>
      <div className="overflow-y-auto" style={{ maxHeight: 340 }}>
        {signals.length === 0 ? (
          <div className="flex items-center justify-center py-14 text-xs" style={{ color: "var(--text-muted)" }}>
            Waiting for signals...
          </div>
        ) : (
          <table className="w-full">
            <thead>
              <tr style={{ borderBottom: "1px solid var(--border)" }}>
                {["Symbol", "Direction", "Net Spread", "Profit", "Qty", "Time"].map((h, i) => (
                  <th
                    key={h}
                    className={`px-4 py-2 text-xs font-normal ${i >= 2 ? "text-right" : "text-left"}`}
                    style={{ color: "var(--text-muted)" }}
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map(s => {
                const net = parseFloat(s.net_spread_pct);
                const profit = parseFloat(s.estimated_profit_usd);
                const ok = net > 0;
                return (
                  <tr
                    key={s.id}
                    className="transition-colors"
                    style={{
                      borderBottom: "1px solid var(--border)",
                      background: ok ? "var(--green-bg)" : "transparent",
                    }}
                    onMouseEnter={e => (e.currentTarget.style.background = "var(--bg-row-hover)")}
                    onMouseLeave={e => (e.currentTarget.style.background = ok ? "var(--green-bg)" : "transparent")}
                  >
                    <td className="px-4 py-2 text-xs font-medium">{s.symbol}</td>
                    <td className="px-4 py-2 text-xs">
                      <span style={{ color: "var(--green)" }}>{s.buy_exchange}</span>
                      <span style={{ color: "var(--text-muted)" }}> → </span>
                      <span style={{ color: "var(--red)" }}>{s.sell_exchange}</span>
                    </td>
                    <td className="px-4 py-2 text-xs text-right font-medium" style={{ color: ok ? "var(--green)" : "var(--red)" }}>
                      {net >= 0 ? "+" : ""}{net.toFixed(3)}%
                    </td>
                    <td className="px-4 py-2 text-xs text-right" style={{ color: profit >= 0 ? "var(--green)" : "var(--red)" }}>
                      ${profit.toFixed(2)}
                    </td>
                    <td className="px-4 py-2 text-xs text-right" style={{ color: "var(--text-secondary)" }}>
                      {parseFloat(s.max_qty).toFixed(4)}
                    </td>
                    <td className="px-4 py-2 text-xs text-right" style={{ color: "var(--text-muted)" }}>
                      {timeAgo(s.receivedAt)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
