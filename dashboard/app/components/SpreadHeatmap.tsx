"use client";
import { useMemo, useState } from "react";
import { EXCHANGES, SYMBOLS, EXCHANGE_META, type Exchange, type Symbol, type TickerState } from "@/app/lib/types";

interface Props {
  tickers: Record<string, TickerState>;
}

function getSpread(bx: Exchange, sx: Exchange, sym: Symbol, t: Record<string, TickerState>) {
  const b = t[`${bx}:${sym}`], s = t[`${sx}:${sym}`];
  if (!b || !s || b.best_ask_usd === 0) return null;
  const gross = ((s.best_bid_usd - b.best_ask_usd) / b.best_ask_usd) * 100;
  const fee = (EXCHANGE_META[bx].fee + EXCHANGE_META[sx].fee) * 100;
  return { gross, net: gross - fee };
}

function cellBg(net: number): string {
  if (net > 0.5) return "rgba(0, 192, 118, 0.25)";
  if (net > 0.1) return "rgba(0, 192, 118, 0.12)";
  if (net > 0) return "rgba(0, 192, 118, 0.05)";
  if (net > -0.3) return "transparent";
  if (net > -0.8) return "rgba(234, 57, 67, 0.06)";
  return "rgba(234, 57, 67, 0.12)";
}

const SHORT_NAMES: Record<Exchange, string> = {
  Binance: "Bin",
  Bybit: "Byb",
  Upbit: "Up",
  Bithumb: "Bit",
};

export default function SpreadHeatmap({ tickers }: Props) {
  const [sym, setSym] = useState<Symbol>("BTC");

  const matrix = useMemo(() => {
    const m: Record<string, ReturnType<typeof getSpread>> = {};
    for (const b of EXCHANGES) for (const s of EXCHANGES)
      m[`${b}:${s}`] = b === s ? null : getSpread(b, s, sym, tickers);
    return m;
  }, [tickers, sym]);

  return (
    <div className="border rounded" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
      <div className="px-3 sm:px-4 py-2 border-b flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
        <span className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>Spread Matrix</span>
        <div className="flex">
          {SYMBOLS.map(s => (
            <button
              key={s}
              onClick={() => setSym(s)}
              className="px-2 py-1 text-xs cursor-pointer transition-colors"
              style={{
                color: sym === s ? "var(--text-primary)" : "var(--text-muted)",
                background: sym === s ? "var(--bg-card-hover)" : "transparent",
                borderRadius: 4,
              }}
            >
              {s}
            </button>
          ))}
        </div>
      </div>
      <div className="p-1.5 sm:p-2 overflow-x-auto">
        <table className="w-full">
          <thead>
            <tr>
              <th className="px-1 sm:px-2 py-1 text-left text-xs font-normal" style={{ color: "var(--text-muted)", width: 60 }}>
                <span className="hidden sm:inline">Buy↓ Sell→</span>
                <span className="sm:hidden">B↓S→</span>
              </th>
              {EXCHANGES.map(ex => (
                <th key={ex} className="px-1 sm:px-2 py-1 text-center text-xs font-normal" style={{ color: "var(--text-muted)" }}>
                  <span className="hidden sm:inline">{ex}</span>
                  <span className="sm:hidden">{SHORT_NAMES[ex]}</span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {EXCHANGES.map(bx => (
              <tr key={bx}>
                <td className="px-1 sm:px-2 py-1 text-xs font-medium">
                  <span className="hidden sm:inline">{bx}</span>
                  <span className="sm:hidden">{SHORT_NAMES[bx]}</span>
                </td>
                {EXCHANGES.map(sx => {
                  if (bx === sx) return <td key={sx} className="px-1 py-2 text-center" style={{ color: "var(--text-muted)" }}>—</td>;
                  const sp = matrix[`${bx}:${sx}`];
                  return (
                    <td key={sx} className="px-1 sm:px-2 py-1.5 sm:py-2 text-center rounded" style={{ background: sp ? cellBg(sp.net) : "transparent" }}>
                      {sp ? (
                        <>
                          <div className="text-xs font-medium" style={{ color: sp.net >= 0 ? "var(--green)" : "var(--red)" }}>
                            {sp.net >= 0 ? "+" : ""}{sp.net.toFixed(2)}%
                          </div>
                          <div className="hidden sm:block" style={{ fontSize: 9, color: "var(--text-muted)", marginTop: 1 }}>
                            {sp.gross >= 0 ? "+" : ""}{sp.gross.toFixed(3)}%
                          </div>
                        </>
                      ) : <span style={{ color: "var(--text-muted)" }}>—</span>}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
