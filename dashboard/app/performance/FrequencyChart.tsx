"use client";
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from "recharts";
import type { HourlyBucket } from "@/app/lib/api";

export default function FrequencyChart({ data }: { data: HourlyBucket[] }) {
  if (data.length === 0) return <Empty />;
  return (
    <ResponsiveContainer width="100%" height={260}>
      <BarChart data={data} margin={{ top: 5, right: 10, left: 0, bottom: 5 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="#1c1c1c" />
        <XAxis dataKey="hour" tick={{ fill: "#5c5c5c", fontSize: 10 }} tickLine={false} axisLine={false} />
        <YAxis tick={{ fill: "#5c5c5c", fontSize: 10 }} tickLine={false} axisLine={false} width={45} />
        <Tooltip
          contentStyle={{ background: "#111", border: "1px solid #1c1c1c", borderRadius: 6, fontSize: 12 }}
          labelStyle={{ color: "#a0a0a0" }}
          itemStyle={{ color: "#5b9cf6" }}
        />
        <Bar dataKey="count" fill="#5b9cf6" radius={[2, 2, 0, 0]} name="信号数" />
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
