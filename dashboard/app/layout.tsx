import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "CEX Arbitrage Dashboard",
  description: "Real-time cross-exchange spot arbitrage monitor",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
