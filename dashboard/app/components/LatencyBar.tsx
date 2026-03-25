"use client";
import type { ExchangeLatencyInfo } from "@/app/lib/types";

interface Props {
  latency: ExchangeLatencyInfo[];
}

function rttColor(ms: number): string {
  if (ms === 0) return "var(--text-muted)";
  if (ms < 100) return "var(--green)";
  if (ms < 300) return "var(--yellow)";
  return "var(--red)";
}

function rttLabel(ms: number): string {
  if (ms === 0) return "—";
  if (ms < 1) return "<1";
  return `${Math.round(ms)}`;
}

export default function LatencyBar({ latency }: Props) {
  if (latency.length === 0) return null;

  return (
    <div
      className="grid grid-cols-2 sm:grid-cols-4 gap-px border-b"
      style={{ background: "var(--border)", borderColor: "var(--border)" }}
    >
      {latency.map((l) => (
        <div
          key={l.exchange}
          className="flex items-center justify-between px-3 sm:px-4 py-1"
          style={{ background: "var(--bg-primary)" }}
        >
          <span className="text-xs" style={{ color: "var(--text-muted)" }}>
            {l.exchange}
          </span>
          <div className="flex items-center gap-1 text-xs">
            <span style={{ color: rttColor(l.last_rtt_ms) }}>
              {rttLabel(l.last_rtt_ms)}ms
            </span>
            {l.samples > 0 && (
              <span className="hidden sm:inline" style={{ color: "var(--text-muted)", fontSize: 10 }}>
                avg {rttLabel(l.avg_rtt_ms)}
              </span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
