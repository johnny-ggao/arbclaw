"use client";
import { useRef, useEffect } from "react";
import { EXCHANGES, SYMBOLS, EXCHANGE_META, type Exchange, type Symbol, type TickerState } from "@/app/lib/types";

interface Props {
  tickers: Record<string, TickerState>;
}

function fmt(n: number, d: number = 2): string {
  if (n === 0) return "—";
  return n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
}

function Cell({ ticker, symbol }: { ticker?: TickerState; symbol: Symbol }) {
  const ref = useRef<HTMLTableCellElement>(null);
  const prev = useRef(0);

  useEffect(() => {
    if (!ticker) return;
    if (prev.current !== 0 && ticker.best_bid_usd !== prev.current) {
      const cls = ticker.best_bid_usd > prev.current ? "flash-up" : "flash-down";
      ref.current?.classList.remove("flash-up", "flash-down");
      void ref.current?.offsetWidth;
      ref.current?.classList.add(cls);
    }
    prev.current = ticker.best_bid_usd;
  }, [ticker?.best_bid_usd]);

  if (!ticker) return <td ref={ref} className="px-2 sm:px-3 py-2 text-center" style={{ color: "var(--text-muted)" }}>—</td>;

  const stale = Date.now() - ticker.updatedAt > 5000;
  const d = symbol === "XRP" ? 4 : 2;

  return (
    <td ref={ref} className="px-2 sm:px-3 py-2 text-right" style={{ opacity: stale ? 0.3 : 1 }}>
      <div className="flex items-baseline justify-end gap-1 sm:gap-3">
        <span className="text-xs" style={{ color: "var(--green)" }}>{fmt(ticker.best_bid_usd, d)}</span>
        <span className="text-xs" style={{ color: "var(--red)" }}>{fmt(ticker.best_ask_usd, d)}</span>
      </div>
      {ticker.quote_currency === "KRW" && (
        <div className="text-right mt-0.5" style={{ color: "var(--text-muted)", fontSize: 10 }}>
          ₩{fmt(ticker.raw_bid, 0)}
        </div>
      )}
    </td>
  );
}

export default function PriceMatrix({ tickers }: Props) {
  return (
    <div className="border rounded" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
      <div className="px-3 sm:px-4 py-2 border-b flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
        <span className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>Price Matrix</span>
        <span className="text-xs" style={{ color: "var(--text-muted)" }}>
          <span style={{ color: "var(--green)" }}>Bid</span>
          {" / "}
          <span style={{ color: "var(--red)" }}>Ask</span>
        </span>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full" style={{ minWidth: 400 }}>
          <thead>
            <tr style={{ borderBottom: "1px solid var(--border)" }}>
              <th className="px-2 sm:px-4 py-1.5 text-left text-xs font-normal" style={{ color: "var(--text-muted)" }}>Ex</th>
              {SYMBOLS.map(s => (
                <th key={s} className="px-2 sm:px-3 py-1.5 text-right text-xs font-medium" style={{ color: "var(--text-secondary)" }}>{s}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {EXCHANGES.map((ex, i) => (
              <tr
                key={ex}
                className="transition-colors"
                style={{
                  borderBottom: i < EXCHANGES.length - 1 ? "1px solid var(--border)" : "none",
                }}
                onMouseEnter={e => (e.currentTarget.style.background = "var(--bg-row-hover)")}
                onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
              >
                <td className="px-2 sm:px-4 py-2 whitespace-nowrap">
                  <span className="inline-block w-2 h-2 rounded-sm mr-1" style={{ background: EXCHANGE_META[ex].color }} />
                  <span className="text-xs font-medium">{ex}</span>
                  <span className="hidden sm:inline ml-1 text-xs" style={{ color: "var(--text-muted)" }}>{EXCHANGE_META[ex].quote}</span>
                </td>
                {SYMBOLS.map(sym => (
                  <Cell key={sym} ticker={tickers[`${ex}:${sym}`]} symbol={sym} />
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
