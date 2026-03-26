function getApiBase(): string {
  if (typeof window === "undefined") return "http://localhost";
  const host = window.location.host;
  const isDev = host.includes("3000");
  if (isDev) return "http://localhost:8765";
  return `${window.location.protocol}//${host}`;
}

export interface HourlyBucket {
  hour: string;
  count: number;
  profit: number;
}

export interface CumulativePoint {
  timestamp: string;
  profit: number;
}

export interface SymbolStats {
  symbol: string;
  count: number;
  profit: number;
  avg_spread: number;
}

export interface PairStats {
  pair: string;
  count: number;
  profit: number;
}

export interface PerformanceStats {
  total_signals: number;
  total_profit: number;
  avg_spread: number;
  annualized_return: number;
  hourly_frequency: HourlyBucket[];
  cumulative_profit: CumulativePoint[];
  by_symbol: SymbolStats[];
  by_pair: PairStats[];
}

export type PeriodKey = "1h" | "24h" | "7d" | "30d" | "all";

export async function fetchPerformance(period: PeriodKey): Promise<PerformanceStats> {
  const res = await fetch(`${getApiBase()}/api/performance?period=${period}`);
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}
