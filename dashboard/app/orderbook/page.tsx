"use client";
import { useState } from "react";
import Link from "next/link";
import { useWebSocket } from "@/app/hooks/useWebSocket";
import type { Exchange, Symbol } from "@/app/lib/types";
import { EXCHANGES, SYMBOLS } from "@/app/lib/types";
import OrderBookCompare from "./OrderBookCompare";

export default function OrderBookPage() {
  const { orderbooks, rate, connected } = useWebSocket();
  const [symbol, setSymbol] = useState<Symbol>("BTC");
  const [leftEx, setLeftEx] = useState<Exchange>("Binance");
  const [rightEx, setRightEx] = useState<Exchange>("Upbit");

  return (
    <div className="min-h-screen flex flex-col" style={{ background: "var(--bg-primary)" }}>
      <header
        className="border-b"
        style={{ background: "var(--bg-secondary)", borderColor: "var(--border)" }}
      >
        <div className="flex items-center justify-between px-3 sm:px-5 h-11">
          <div className="flex items-center gap-2 sm:gap-5">
            <span className="text-sm font-bold tracking-wide" style={{ color: "var(--text-primary)" }}>
              CEX Arb
            </span>
            <nav className="flex items-center gap-1 text-xs">
              <Link href="/" className="px-2 sm:px-3 py-1 rounded transition-colors" style={{ color: "var(--text-muted)" }}>
                监控
              </Link>
              <Link href="/performance" className="px-2 sm:px-3 py-1 rounded transition-colors" style={{ color: "var(--text-muted)" }}>
                绩效
              </Link>
              <span className="px-2 sm:px-3 py-1 rounded" style={{ color: "var(--text-primary)", background: "var(--bg-card-hover)" }}>
                盘口
              </span>
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
              <span className="hidden sm:inline">{connected ? "Live" : "Off"}</span>
            </div>
          </div>
        </div>
      </header>

      <main className="flex-1 p-3 sm:p-4 lg:p-6 max-w-[1200px] mx-auto w-full">
        {/* Selectors */}
        <div className="flex flex-wrap items-center gap-3 mb-4">
          {/* Symbol */}
          <div className="flex rounded overflow-hidden border" style={{ borderColor: "var(--border)" }}>
            {SYMBOLS.map((s) => (
              <button
                key={s}
                onClick={() => setSymbol(s)}
                className="px-3 py-1.5 text-xs cursor-pointer transition-colors"
                style={{
                  background: symbol === s ? "var(--bg-card-hover)" : "transparent",
                  color: symbol === s ? "var(--text-primary)" : "var(--text-muted)",
                  borderRight: "1px solid var(--border)",
                }}
              >
                {s}
              </button>
            ))}
          </div>

          {/* Left exchange */}
          <select
            value={leftEx}
            onChange={(e) => setLeftEx(e.target.value as Exchange)}
            className="px-3 py-1.5 text-xs rounded border cursor-pointer"
            style={{
              background: "var(--bg-secondary)",
              borderColor: "var(--border)",
              color: "var(--text-primary)",
            }}
          >
            {EXCHANGES.map((ex) => (
              <option key={ex} value={ex}>{ex}</option>
            ))}
          </select>

          <span className="text-xs" style={{ color: "var(--text-muted)" }}>vs</span>

          {/* Right exchange */}
          <select
            value={rightEx}
            onChange={(e) => setRightEx(e.target.value as Exchange)}
            className="px-3 py-1.5 text-xs rounded border cursor-pointer"
            style={{
              background: "var(--bg-secondary)",
              borderColor: "var(--border)",
              color: "var(--text-primary)",
            }}
          >
            {EXCHANGES.map((ex) => (
              <option key={ex} value={ex}>{ex}</option>
            ))}
          </select>
        </div>

        <OrderBookCompare
          leftOB={orderbooks[`${leftEx}:${symbol}`]}
          rightOB={orderbooks[`${rightEx}:${symbol}`]}
          leftEx={leftEx}
          rightEx={rightEx}
          symbol={symbol}
          rate={rate}
        />
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
