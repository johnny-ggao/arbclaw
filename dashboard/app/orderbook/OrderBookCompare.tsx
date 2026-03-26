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

interface Level {
  priceUsd: number;
  qty: number;
  raw: string;
}

const DEPTH = 5;

function normalize(
  p: number, q: number,
  quote: "USDT" | "KRW",
  rate: RateInfo | null,
): Level {
  if (quote === "KRW" && rate) {
    return {
      priceUsd: (p / rate.krw_per_usdt) * rate.usdt_per_usd,
      qty: q,
      raw: `₩${p.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
    };
  }
  return { priceUsd: p, qty: q, raw: "" };
}

function priceFmt(n: number): string {
  if (n === 0) return "—";
  const d = n > 100 ? 2 : 4;
  return n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
}

function qtyFmt(n: number): string {
  if (n === 0) return "";
  if (n >= 1) return n.toFixed(4);
  if (n >= 0.01) return n.toFixed(6);
  return n.toFixed(8);
}

const EMPTY: Level = { priceUsd: 0, qty: 0, raw: "" };

export default function OrderBookCompare({
  leftOB, rightOB, leftEx, rightEx, symbol, rate,
}: Props) {
  const data = useMemo(() => {
    const lQ = EXCHANGE_META[leftEx].quote;
    const rQ = EXCHANGE_META[rightEx].quote;

    // asks: sorted low→high, we display top rows = highest ask, bottom = lowest (best) ask
    const lAsks = (leftOB?.asks || []).slice(0, DEPTH).map(l => normalize(l.price, l.qty, lQ, rate));
    const rAsks = (rightOB?.asks || []).slice(0, DEPTH).map(l => normalize(l.price, l.qty, rQ, rate));
    // bids: sorted high→low, top = best bid
    const lBids = (leftOB?.bids || []).slice(0, DEPTH).map(l => normalize(l.price, l.qty, lQ, rate));
    const rBids = (rightOB?.bids || []).slice(0, DEPTH).map(l => normalize(l.price, l.qty, rQ, rate));

    // Pad to DEPTH
    while (lAsks.length < DEPTH) lAsks.push(EMPTY);
    while (rAsks.length < DEPTH) rAsks.push(EMPTY);
    while (lBids.length < DEPTH) lBids.push(EMPTY);
    while (rBids.length < DEPTH) rBids.push(EMPTY);

    // Reverse asks so highest at top, best ask at bottom (adjacent to spread bar)
    const lAsksDisplay = [...lAsks].reverse();
    const rAsksDisplay = [...rAsks].reverse();

    const allQty = [...lAsks, ...rAsks, ...lBids, ...rBids].map(l => l.qty).filter(q => q > 0);
    const maxQty = allQty.length > 0 ? Math.max(...allQty) : 1;

    // Cross-exchange spread
    const lBestAsk = lAsks[0]?.priceUsd || 0;
    const rBestBid = rBids[0]?.priceUsd || 0;
    const rBestAsk = rAsks[0]?.priceUsd || 0;
    const lBestBid = lBids[0]?.priceUsd || 0;

    // Internal spreads
    const lSpread = lBestAsk > 0 && lBestBid > 0 ? lBestAsk - lBestBid : 0;
    const rSpread = rBestAsk > 0 && rBestBid > 0 ? rBestAsk - rBestBid : 0;

    // Cross spreads: buy left sell right, buy right sell left
    const crossLR = lBestAsk > 0 ? ((rBestBid - lBestAsk) / lBestAsk) * 100 : 0;
    const crossRL = rBestAsk > 0 ? ((lBestBid - rBestAsk) / rBestAsk) * 100 : 0;

    return { lAsksDisplay, rAsksDisplay, lBids, rBids, maxQty, lSpread, rSpread, crossLR, crossRL };
  }, [leftOB, rightOB, leftEx, rightEx, rate]);

  const noData = !leftOB && !rightOB;

  if (noData) {
    return (
      <div
        className="border rounded flex items-center justify-center py-20"
        style={{ borderColor: "var(--border)", background: "var(--bg-secondary)", color: "var(--text-muted)" }}
      >
        <span className="text-sm">Waiting for orderbook data...</span>
      </div>
    );
  }

  const ROW_H = 32;
  const barWidth = (qty: number) => data.maxQty > 0 ? `${(qty / data.maxQty) * 100}%` : "0%";

  return (
    <div className="border rounded" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
      {/* Header */}
      <div className="grid grid-cols-2 border-b" style={{ borderColor: "var(--border)" }}>
        <div className="px-3 py-2 flex items-center justify-between border-r" style={{ borderColor: "var(--border)" }}>
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-sm" style={{ background: EXCHANGE_META[leftEx].color }} />
            <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>{leftEx}</span>
            <span className="text-xs" style={{ color: "var(--text-muted)" }}>{EXCHANGE_META[leftEx].quote}</span>
          </div>
          <span className="text-xs" style={{ color: data.crossLR > 0 ? "var(--green)" : "var(--red)" }}>
            Buy→Sell: {data.crossLR >= 0 ? "+" : ""}{data.crossLR.toFixed(3)}%
          </span>
        </div>
        <div className="px-3 py-2 flex items-center justify-between">
          <span className="text-xs" style={{ color: data.crossRL > 0 ? "var(--green)" : "var(--red)" }}>
            Buy→Sell: {data.crossRL >= 0 ? "+" : ""}{data.crossRL.toFixed(3)}%
          </span>
          <div className="flex items-center gap-2">
            <span className="text-xs" style={{ color: "var(--text-muted)" }}>{EXCHANGE_META[rightEx].quote}</span>
            <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>{rightEx}</span>
            <span className="w-2 h-2 rounded-sm" style={{ background: EXCHANGE_META[rightEx].color }} />
          </div>
        </div>
      </div>

      {/* Column headers */}
      <div className="grid grid-cols-2 border-b" style={{ borderColor: "var(--border)" }}>
        <div className="flex justify-between px-3 py-1 border-r text-xs" style={{ borderColor: "var(--border)", color: "var(--text-muted)" }}>
          <span>Qty</span><span>Price (USD)</span>
        </div>
        <div className="flex justify-between px-3 py-1 text-xs" style={{ color: "var(--text-muted)" }}>
          <span>Price (USD)</span><span>Qty</span>
        </div>
      </div>

      {/* ASKS — highest at top, best ask at bottom */}
      {data.lAsksDisplay.map((_, i) => {
        const lv = data.lAsksDisplay[i];
        const rv = data.rAsksDisplay[i];
        return (
          <div key={`ask-${i}`} className="grid grid-cols-2" style={{ height: ROW_H }}>
            {/* Left ask: volume bar grows right-to-left */}
            <div className="relative flex items-center justify-between px-3 border-r" style={{ borderColor: "var(--border)" }}>
              <div className="absolute top-0 bottom-0 right-0" style={{ width: barWidth(lv.qty), background: "rgba(234,57,67,0.10)" }} />
              <span className="relative z-10 text-xs font-mono" style={{ color: "var(--text-secondary)" }}>{qtyFmt(lv.qty)}</span>
              <div className="relative z-10 text-right">
                <span className="text-xs font-mono" style={{ color: lv.priceUsd > 0 ? "var(--red)" : "var(--text-muted)" }}>
                  {priceFmt(lv.priceUsd)}
                </span>
                {lv.raw && <div style={{ fontSize: 9, color: "var(--text-muted)", lineHeight: 1 }}>{lv.raw}</div>}
              </div>
            </div>
            {/* Right ask: volume bar grows left-to-right */}
            <div className="relative flex items-center justify-between px-3">
              <div className="absolute top-0 bottom-0 left-0" style={{ width: barWidth(rv.qty), background: "rgba(234,57,67,0.10)" }} />
              <div className="relative z-10">
                <span className="text-xs font-mono" style={{ color: rv.priceUsd > 0 ? "var(--red)" : "var(--text-muted)" }}>
                  {priceFmt(rv.priceUsd)}
                </span>
                {rv.raw && <div style={{ fontSize: 9, color: "var(--text-muted)", lineHeight: 1 }}>{rv.raw}</div>}
              </div>
              <span className="relative z-10 text-xs font-mono" style={{ color: "var(--text-secondary)" }}>{qtyFmt(rv.qty)}</span>
            </div>
          </div>
        );
      })}

      {/* SPREAD BAR — divider between asks and bids */}
      <div className="grid grid-cols-2 border-y" style={{ borderColor: "var(--border)", background: "var(--bg-primary)" }}>
        <div className="px-3 py-1.5 text-center border-r" style={{ borderColor: "var(--border)" }}>
          <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
            {data.lSpread > 0 ? priceFmt(data.lSpread) : "—"}
          </span>
          <span className="ml-1 text-xs" style={{ color: "var(--text-muted)" }}>spread</span>
        </div>
        <div className="px-3 py-1.5 text-center">
          <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
            {data.rSpread > 0 ? priceFmt(data.rSpread) : "—"}
          </span>
          <span className="ml-1 text-xs" style={{ color: "var(--text-muted)" }}>spread</span>
        </div>
      </div>

      {/* BIDS — best bid at top */}
      {data.lBids.map((_, i) => {
        const lv = data.lBids[i];
        const rv = data.rBids[i];
        return (
          <div key={`bid-${i}`} className="grid grid-cols-2" style={{ height: ROW_H }}>
            {/* Left bid: volume bar grows right-to-left */}
            <div className="relative flex items-center justify-between px-3 border-r" style={{ borderColor: "var(--border)" }}>
              <div className="absolute top-0 bottom-0 right-0" style={{ width: barWidth(lv.qty), background: "rgba(0,192,118,0.10)" }} />
              <span className="relative z-10 text-xs font-mono" style={{ color: "var(--text-secondary)" }}>{qtyFmt(lv.qty)}</span>
              <div className="relative z-10 text-right">
                <span className="text-xs font-mono" style={{ color: lv.priceUsd > 0 ? "var(--green)" : "var(--text-muted)" }}>
                  {priceFmt(lv.priceUsd)}
                </span>
                {lv.raw && <div style={{ fontSize: 9, color: "var(--text-muted)", lineHeight: 1 }}>{lv.raw}</div>}
              </div>
            </div>
            {/* Right bid: volume bar grows left-to-right */}
            <div className="relative flex items-center justify-between px-3">
              <div className="absolute top-0 bottom-0 left-0" style={{ width: barWidth(rv.qty), background: "rgba(0,192,118,0.10)" }} />
              <div className="relative z-10">
                <span className="text-xs font-mono" style={{ color: rv.priceUsd > 0 ? "var(--green)" : "var(--text-muted)" }}>
                  {priceFmt(rv.priceUsd)}
                </span>
                {rv.raw && <div style={{ fontSize: 9, color: "var(--text-muted)", lineHeight: 1 }}>{rv.raw}</div>}
              </div>
              <span className="relative z-10 text-xs font-mono" style={{ color: "var(--text-secondary)" }}>{qtyFmt(rv.qty)}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
