"use client";
import { useWebSocket } from "@/app/hooks/useWebSocket";
import StatusBar from "@/app/components/StatusBar";
import StatsCards from "@/app/components/StatsCards";
import PriceMatrix from "@/app/components/PriceMatrix";
import SpreadHeatmap from "@/app/components/SpreadHeatmap";
import SignalTable from "@/app/components/SignalTable";
import LatencyBar from "@/app/components/LatencyBar";

export default function Home() {
  const { tickers, signals, rate, latency, connected, tickCount } = useWebSocket();

  return (
    <div className="min-h-screen flex flex-col" style={{ background: "var(--bg-primary)" }}>
      <StatusBar connected={connected} tickCount={tickCount} rate={rate} />
      <LatencyBar latency={latency} />
      <StatsCards tickers={tickers} signalCount={signals.length} rate={rate} />
      <main className="flex-1 p-3 lg:p-4 space-y-3">
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
          <PriceMatrix tickers={tickers} />
          <SpreadHeatmap tickers={tickers} />
        </div>
        <SignalTable signals={signals} />
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
