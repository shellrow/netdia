import { Ipv4Net, Ipv6Net, toDate } from "../types/net.ts";

// Format helpers
export function nv(v?: string | number | null): string {
  if (v === null || v === undefined) return "-";
  const s = String(v).trim();
  return s.length ? s : "-";
}

export function fmtIfType(t?: string) {
  return t ?? "Unknown";
}

export function fmtDate(ts: unknown): string {
  if (ts == null) return "-";
  const d = toDate(ts);
  return isNaN(+d) ? "-" : d.toLocaleString();
}

export function fmtMs(v?: number | null): string {
  if (v == null) return "-";
  return `${v} ms`;
}

export function fmtBps(v: number): string {
  if (!isFinite(v) || v <= 0) return "0 bps";
  const u = ["bps", "Kbps", "Mbps", "Gbps", "Tbps"];
  let i = 0;
  let n = v;
  while (n >= 1000 && i < u.length - 1) { n /= 1000; i++; }
  return `${n.toFixed(n >= 100 ? 0 : n >= 10 ? 1 : 2)} ${u[i]}`;
}

export function fmtBytesPerSec(v: number): string {
  if (!isFinite(v) || v <= 0) return "0 B/s";
  const units = ["B/s", "kB/s", "MB/s", "GB/s", "TB/s"];
  let n = v;
  let i = 0;
  while (n >= 1000 && i < units.length - 1) {
    n /= 1000;
    i++;
  }
  const decimals = n >= 100 ? 0 : n >= 10 ? 1 : 2;
  return `${n.toFixed(decimals)} ${units[i]}`;
}

export function fmtBytes(v: number): string {
  if (!isFinite(v) || v <= 0) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB", "PB"];
  let i = 0;
  let n = v;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(n >= 100 ? 0 : n >= 10 ? 1 : 2)} ${u[i]}`;
}

export function hexFlags(flags?: number) {
  if (flags == null) return "0x0";
  return "0x" + flags.toString(16).toUpperCase();
}

export function severityByOper(s?: string) {
  const v = (s ?? "").toLowerCase();
  return v === "up" ? "success" : v === "down" ? "danger" : "secondary";
}

export function shortenIpList(list?: (Ipv4Net | Ipv6Net)[]): string {
  if (!list || list.length === 0) return "-";

  const first = typeof list[0] === "string"
    ? list[0]
    : `${list[0].addr}/${list[0].prefix_len}`;

  if (list.length === 1) return first;

  return `${first} + ${list.length - 1}`;
}
