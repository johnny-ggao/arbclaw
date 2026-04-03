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
  const profitable = signals.filter(s => parseFloat(s.vwap_spread_pct ?? s.gross_spread_pct) >= 2.0);
  const rows = profitable.length > 0 ? profitable.slice(0, 50) : signals.slice(0, 50);

  return (
    <div className="border rounded" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
      <div className="px-3 sm:px-4 py-2 border-b flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
        <span className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>Signals</span>
        <span className="text-xs" style={{ color: profitable.length > 0 ? "var(--green)" : "var(--text-muted)" }}>
          {profitable.length > 0 ? `${profitable.length} profitable` : "no opportunities"}
        </span>
      </div>
      <div className="overflow-x-auto" style={{ maxHeight: 340 }}>
        {signals.length === 0 ? (
          <div className="flex items-center justify-center py-14 text-xs" style={{ color: "var(--text-muted)" }}>
            Waiting for signals...
          </div>
        ) : (
          <table className="w-full" style={{ minWidth: 360 }}>
            <thead>
              <tr style={{ borderBottom: "1px solid var(--border)" }}>
                <th className="px-2 sm:px-4 py-1.5 text-left text-xs font-normal" style={{ color: "var(--text-muted)" }}>Pair</th>
                <th className="px-2 sm:px-4 py-1.5 text-right text-xs font-normal" style={{ color: "var(--text-muted)" }}>VWAP</th>
                <th className="px-2 sm:px-4 py-1.5 text-right text-xs font-normal hidden sm:table-cell" style={{ color: "var(--text-muted)" }}>BBO</th>
                <th className="px-2 sm:px-4 py-1.5 text-right text-xs font-normal hidden sm:table-cell" style={{ color: "var(--text-muted)" }}>Profit</th>
                <th className="px-2 sm:px-4 py-1.5 text-right text-xs font-normal hidden sm:table-cell" style={{ color: "var(--text-muted)" }}>Qty</th>
                <th className="px-2 sm:px-4 py-1.5 text-right text-xs font-normal" style={{ color: "var(--text-muted)" }}>Time</th>
              </tr>
            </thead>
            <tbody>
              {rows.map(s => {
                const vwap = parseFloat(s.vwap_spread_pct ?? s.gross_spread_pct);
                const bbo = parseFloat(s.gross_spread_pct);
                const profit = parseFloat(s.estimated_profit_usd);
                const ok = vwap >= 2.0;
                return (
                  <tr
                    key={s.id}
                    className="transition-colors"
                    style={{
                      borderBottom: "1px solid var(--border)",
                      background: ok ? "var(--green-bg)" : "transparent",
                    }}
                  >
                    <td className="px-2 sm:px-4 py-1.5 text-xs">
                      <span className="font-medium">{s.symbol}</span>
                      <span className="ml-1" style={{ color: "var(--text-muted)" }}>
                        {s.buy_exchange.slice(0, 3)}→{s.sell_exchange.slice(0, 3)}
                      </span>
                    </td>
                    <td className="px-2 sm:px-4 py-1.5 text-xs text-right font-medium" style={{ color: ok ? "var(--green)" : "var(--red)" }}>
                      {vwap >= 0 ? "+" : ""}{vwap.toFixed(3)}%
                    </td>
                    <td className="px-2 sm:px-4 py-1.5 text-xs text-right hidden sm:table-cell" style={{ color: bbo >= 0 ? "var(--green)" : "var(--red)" }}>
                      {bbo >= 0 ? "+" : ""}{bbo.toFixed(3)}%
                    </td>
                    <td className="px-2 sm:px-4 py-1.5 text-xs text-right hidden sm:table-cell" style={{ color: profit >= 0 ? "var(--green)" : "var(--red)" }}>
                      ${profit.toFixed(2)}
                    </td>
                    <td className="px-2 sm:px-4 py-1.5 text-xs text-right hidden sm:table-cell" style={{ color: "var(--text-secondary)" }}>
                      {parseFloat(s.max_qty).toFixed(4)}
                    </td>
                    <td className="px-2 sm:px-4 py-1.5 text-xs text-right" style={{ color: "var(--text-muted)" }}>
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
