"use client";
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from "recharts";
import type { PairStats } from "@/app/lib/api";

export default function PairChart({ data }: { data: PairStats[] }) {
  if (data.length === 0) return <Empty />;

  const sorted = [...data].sort((a, b) => b.count - a.count).slice(0, 10);

  return (
    <ResponsiveContainer width="100%" height={260}>
      <BarChart data={sorted} margin={{ top: 5, right: 10, left: 0, bottom: 5 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="#1c1c1c" />
        <XAxis dataKey="pair" tick={{ fill: "#a0a0a0", fontSize: 10 }} tickLine={false} axisLine={false} />
        <YAxis tick={{ fill: "#5c5c5c", fontSize: 10 }} tickLine={false} axisLine={false} width={50} />
        <Tooltip
          contentStyle={{ background: "#111", border: "1px solid #1c1c1c", borderRadius: 6, fontSize: 12 }}
          labelStyle={{ color: "#a0a0a0" }}
          formatter={(value, name) => {
            if (name === "count") return [value, "信号数"];
            return [`$${Number(value).toFixed(2)}`, "利润"];
          }}
        />
        <Bar dataKey="count" fill="#00c076" radius={[2, 2, 0, 0]} name="count" />
      </BarChart>
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
