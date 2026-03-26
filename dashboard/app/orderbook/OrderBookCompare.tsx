"use client";
import { useMemo } from "react";
import type { Exchange, Symbol } from "@/app/lib/types";
import { EXCHANGE_META } from "@/app/lib/types";
import type { OrderBookState } from "@/app/hooks/useWebSocket";

interface RateInfo {
  krw_per_usdt: number;
  usdt_per_usd: number;
  krw_per_usd: number;
  source: string;
}

interface Props {
  leftOB?: OrderBookState;
  rightOB?: OrderBookState;
  leftEx: Exchange;
  rightEx: Exchange;
  symbol: Symbol;
  rate: RateInfo | null;
}

interface NormalizedLevel {
  price: number;
  priceUsd: number;
  qty: number;
  raw: string;
}

function normalizeLevel(
  p: number,
  q: number,
  quote: "USDT" | "KRW",
  rate: RateInfo | null,
): NormalizedLevel {
  let priceUsd = p;
  let raw = "";
  if (quote === "KRW" && rate) {
    priceUsd = p / rate.krw_per_usdt * rate.usdt_per_usd;
    raw = `₩${p.toLocaleString("en-US", { maximumFractionDigits: 0 })}`;
  }
  return { price: p, priceUsd, qty: q, raw };
}

function fmt(n: number, d: number): string {
  return n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
}

function qtyFmt(n: number): string {
  if (n >= 1) return fmt(n, 4);
  if (n >= 0.01) return fmt(n, 6);
  return n.toFixed(8);
}

function Side({
  levels,
  side,
  maxQty,
  color,
}: {
  levels: NormalizedLevel[];
  side: "bid" | "ask";
  maxQty: number;
  color: string;
}) {
  return (
    <div className="space-y-px">
      {levels.map((l, i) => {
        const barPct = maxQty > 0 ? (l.qty / maxQty) * 100 : 0;
        return (
          <div
            key={i}
            className="relative flex items-center justify-between px-2 sm:px-3 py-1"
            style={{ background: "var(--bg-primary)" }}
          >
            {/* Volume bar */}
            <div
              className="absolute top-0 bottom-0"
              style={{
                [side === "bid" ? "right" : "left"]: 0,
                width: `${Math.min(barPct, 100)}%`,
                background: color,
                opacity: 0.08,
              }}
            />
            {/* Content */}
            <div className="relative z-10 flex items-center justify-between w-full gap-2">
              {side === "bid" ? (
                <>
                  <span className="text-xs font-mono" style={{ color: "var(--text-secondary)" }}>
                    {qtyFmt(l.qty)}
                  </span>
                  <div className="text-right">
                    <span className="text-xs font-mono font-medium" style={{ color }}>
                      {fmt(l.priceUsd, l.priceUsd > 100 ? 2 : 4)}
                    </span>
                    {l.raw && (
                      <div style={{ fontSize: 9, color: "var(--text-muted)" }}>{l.raw}</div>
                    )}
                  </div>
                </>
              ) : (
                <>
                  <div>
                    <span className="text-xs font-mono font-medium" style={{ color }}>
                      {fmt(l.priceUsd, l.priceUsd > 100 ? 2 : 4)}
                    </span>
                    {l.raw && (
                      <div style={{ fontSize: 9, color: "var(--text-muted)" }}>{l.raw}</div>
                    )}
                  </div>
                  <span className="text-xs font-mono" style={{ color: "var(--text-secondary)" }}>
                    {qtyFmt(l.qty)}
                  </span>
                </>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

export default function OrderBookCompare({
  leftOB,
  rightOB,
  leftEx,
  rightEx,
  symbol,
  rate,
}: Props) {
  const { leftAsks, leftBids, rightAsks, rightBids, maxQty, spread } = useMemo(() => {
    const lQuote = EXCHANGE_META[leftEx].quote;
    const rQuote = EXCHANGE_META[rightEx].quote;

    const lBids = (leftOB?.bids || []).map(l => normalizeLevel(l.price, l.qty, lQuote, rate));
    const lAsks = (leftOB?.asks || []).map(l => normalizeLevel(l.price, l.qty, lQuote, rate));
    const rBids = (rightOB?.bids || []).map(l => normalizeLevel(l.price, l.qty, rQuote, rate));
    const rAsks = (rightOB?.asks || []).map(l => normalizeLevel(l.price, l.qty, rQuote, rate));

    const allQty = [...lBids, ...lAsks, ...rBids, ...rAsks].map(l => l.qty);
    const mq = allQty.length > 0 ? Math.max(...allQty) : 1;

    // Spread: best ask left vs best bid right, and vice versa
    let sp = null;
    if (lAsks.length > 0 && rBids.length > 0 && lBids.length > 0 && rAsks.length > 0) {
      const lBestAsk = lAsks[0].priceUsd;
      const rBestBid = rBids[0].priceUsd;
      const rBestAsk = rAsks[0].priceUsd;
      const lBestBid = lBids[0].priceUsd;
      const s1 = lBestAsk > 0 ? ((rBestBid - lBestAsk) / lBestAsk) * 100 : 0;
      const s2 = rBestAsk > 0 ? ((lBestBid - rBestAsk) / rBestAsk) * 100 : 0;
      sp = { leftToRight: s1, rightToLeft: s2 };
    }

    return {
      leftAsks: lAsks.slice().reverse(), // show highest ask at top
      leftBids: lBids,
      rightAsks: rAsks.slice().reverse(),
      rightBids: rBids,
      maxQty: mq,
      spread: sp,
    };
  }, [leftOB, rightOB, leftEx, rightEx, rate]);

  const noData = !leftOB && !rightOB;

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      {/* Left exchange */}
      <div className="border rounded" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
        <div className="px-3 py-2 border-b flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-sm" style={{ background: EXCHANGE_META[leftEx].color }} />
            <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
              {leftEx} <span style={{ color: "var(--text-muted)" }}>{symbol}/{EXCHANGE_META[leftEx].quote}</span>
            </span>
          </div>
          {spread && (
            <span className="text-xs" style={{ color: spread.leftToRight > 0 ? "var(--green)" : "var(--red)" }}>
              →{rightEx}: {spread.leftToRight >= 0 ? "+" : ""}{spread.leftToRight.toFixed(3)}%
            </span>
          )}
        </div>
        {leftOB ? (
          <div className="py-1">
            <div className="px-3 py-0.5 flex justify-between text-xs" style={{ color: "var(--text-muted)" }}>
              <span>Qty</span><span>Ask (USD)</span>
            </div>
            <Side levels={leftAsks} side="ask" maxQty={maxQty} color="var(--red)" />
            <div className="px-3 py-1.5 text-center border-y" style={{ borderColor: "var(--border)" }}>
              <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
                {leftBids.length > 0 && leftAsks.length > 0
                  ? fmt((leftAsks[leftAsks.length - 1]?.priceUsd || 0) - (leftBids[0]?.priceUsd || 0), 2)
                  : "—"
                }
              </span>
              <span className="ml-1 text-xs" style={{ color: "var(--text-muted)" }}>spread</span>
            </div>
            <div className="px-3 py-0.5 flex justify-between text-xs" style={{ color: "var(--text-muted)" }}>
              <span>Qty</span><span>Bid (USD)</span>
            </div>
            <Side levels={leftBids} side="bid" maxQty={maxQty} color="var(--green)" />
          </div>
        ) : (
          <div className="flex items-center justify-center py-16 text-xs" style={{ color: "var(--text-muted)" }}>
            {noData ? "Waiting for data..." : "No data"}
          </div>
        )}
      </div>

      {/* Right exchange */}
      <div className="border rounded" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
        <div className="px-3 py-2 border-b flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-sm" style={{ background: EXCHANGE_META[rightEx].color }} />
            <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
              {rightEx} <span style={{ color: "var(--text-muted)" }}>{symbol}/{EXCHANGE_META[rightEx].quote}</span>
            </span>
          </div>
          {spread && (
            <span className="text-xs" style={{ color: spread.rightToLeft > 0 ? "var(--green)" : "var(--red)" }}>
              →{leftEx}: {spread.rightToLeft >= 0 ? "+" : ""}{spread.rightToLeft.toFixed(3)}%
            </span>
          )}
        </div>
        {rightOB ? (
          <div className="py-1">
            <div className="px-3 py-0.5 flex justify-between text-xs" style={{ color: "var(--text-muted)" }}>
              <span>Ask (USD)</span><span>Qty</span>
            </div>
            <Side levels={rightAsks} side="ask" maxQty={maxQty} color="var(--red)" />
            <div className="px-3 py-1.5 text-center border-y" style={{ borderColor: "var(--border)" }}>
              <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
                {rightBids.length > 0 && rightAsks.length > 0
                  ? fmt((rightAsks[rightAsks.length - 1]?.priceUsd || 0) - (rightBids[0]?.priceUsd || 0), 2)
                  : "—"
                }
              </span>
              <span className="ml-1 text-xs" style={{ color: "var(--text-muted)" }}>spread</span>
            </div>
            <div className="px-3 py-0.5 flex justify-between text-xs" style={{ color: "var(--text-muted)" }}>
              <span>Bid (USD)</span><span>Qty</span>
            </div>
            <Side levels={rightBids} side="ask" maxQty={maxQty} color="var(--green)" />
          </div>
        ) : (
          <div className="flex items-center justify-center py-16 text-xs" style={{ color: "var(--text-muted)" }}>
            {noData ? "Waiting for data..." : "No data"}
          </div>
        )}
      </div>
    </div>
  );
}
