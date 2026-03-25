"use client";
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from "recharts";
import type { CumulativePoint } from "@/app/lib/api";

export default function CumulativeChart({ data }: { data: CumulativePoint[] }) {
  if (data.length === 0) return <Empty />;

  const formatted = data.map((d) => ({
    time: new Date(d.timestamp).toLocaleString("en-US", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }),
    profit: d.profit,
  }));

  return (
    <ResponsiveContainer width="100%" height={260}>
      <AreaChart data={formatted} margin={{ top: 5, right: 10, left: 0, bottom: 5 }}>
        <defs>
          <linearGradient id="profitGrad" x1="0" y1="0" x2="0" y2="1">
            <stop offset="5%" stopColor="#00c076" stopOpacity={0.3} />
            <stop offset="95%" stopColor="#00c076" stopOpacity={0} />
          </linearGradient>
        </defs>
        <CartesianGrid strokeDasharray="3 3" stroke="#1c1c1c" />
        <XAxis dataKey="time" tick={{ fill: "#5c5c5c", fontSize: 10 }} tickLine={false} axisLine={false} />
        <YAxis tick={{ fill: "#5c5c5c", fontSize: 10 }} tickLine={false} axisLine={false} width={55} />
        <Tooltip
          contentStyle={{ background: "#111", border: "1px solid #1c1c1c", borderRadius: 6, fontSize: 12 }}
          labelStyle={{ color: "#a0a0a0" }}
          formatter={(value) => [`$${Number(value).toFixed(2)}`, "累计利润"]}
        />
        <Area type="monotone" dataKey="profit" stroke="#00c076" fillOpacity={1} fill="url(#profitGrad)" strokeWidth={2} />
      </AreaChart>
    </ResponsiveContainer>
  );
}

function Empty() {
  return (
    <div className="flex items-center justify-center" style={{ height: 260, color: "var(--text-muted)", fontSize: 12 }}>
      暂无数据
    </div>
  );
}
