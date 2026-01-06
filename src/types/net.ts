//! network related types

// rust std types
export type SystemTimeJson =
  | string
  | number
  | { secs_since_epoch: number; nanos_since_epoch?: number }
  | { tv_sec: number; tv_nsec?: number };

export type IpAddr = string;
export type MacAddr = string;
export type Ipv4Net = string | { addr: string; prefix_len: number };
export type Ipv6Net = string | { addr: string; prefix_len: number };

export function cidr(addr: string, prefix_len: number): string {
  return `${addr}/${prefix_len}`;
}

export interface Host {
  ip: IpAddr;
  hostname: string;
}

export interface NetworkDevice {
  mac_addr: string;
  ipv4: string[];
  ipv6: string[];
}

export interface TrafficStats {
  rx_bytes: number;
  tx_bytes: number;
  rx_bytes_per_sec: number;
  tx_bytes_per_sec: number;
  timestamp: SystemTimeJson;
}

export interface NetworkInterface {
  index: number;
  name: string;
  display_name: string;
  friendly_name?: string | null;
  description?: string | null;
  if_type: string;
  mac_addr?: MacAddr | null;
  ipv4: Ipv4Net[];
  ipv6: Ipv6Net[];
  ipv6_scope_ids: number[];
  flags: number;
  oper_state: string;
  transmit_speed?: number | null; // bit per second
  receive_speed?: number | null;  // bit per second
  stats: TrafficStats;
  gateway?: NetworkDevice | null;
  dns_servers?: IpAddr[];
  default?: boolean;
  mtu?: number | null;
}

// Helpers
export function ipListToString(xs?: (Ipv4Net | Ipv6Net)[]): string {
  if (!xs || xs.length === 0) return "";
  return xs
    .map((v) =>
      typeof v === "string"
        ? v
        : v && typeof v === "object"
        ? `${v.addr}/${"prefix_len" in v ? v.prefix_len : ""}`.replace(/\/$/, "")
        : "",
    )
    .filter(Boolean)
    .join(", ");
}

export function formatBps(v: number): string {
  if (!isFinite(v) || v <= 0) return "0 bps";
  const u = ["bps", "Kbps", "Mbps", "Gbps", "Tbps"];
  let i = 0;
  let n = v;
  while (n >= 1000 && i < u.length - 1) { n /= 1000; i++; }
  return `${n.toFixed(n >= 100 ? 0 : n >= 10 ? 1 : 2)} ${u[i]}`;
}

export function formatBytesPerSec(v: number): string {
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

export function formatBytes(v: number): string {
  if (!isFinite(v) || v <= 0) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB", "PB"];
  let i = 0;
  let n = v;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(n >= 100 ? 0 : n >= 10 ? 1 : 2)} ${u[i]}`;
}

export function toDate(ts: unknown): Date {
  if (ts == null) return new Date(NaN);

  if (typeof ts === "string" || typeof ts === "number") {
    return new Date(ts);
  }

  if (typeof ts === "object") {
    // SystemTimeJson: { secs_since_epoch, nanos_since_epoch? }
    if ("secs_since_epoch" in ts) {
      const o = ts as { secs_since_epoch: number; nanos_since_epoch?: number | null };
      const ms =
        o.secs_since_epoch * 1000 + Math.round((o.nanos_since_epoch ?? 0) / 1e6);
      return new Date(ms);
    }

    // timespec: { tv_sec, tv_nsec? }
    if ("tv_sec" in ts) {
      const o = ts as { tv_sec: number; tv_nsec?: number | null };
      const ms = o.tv_sec * 1000 + Math.round((o.tv_nsec ?? 0) / 1e6);
      return new Date(ms);
    }
  }

  return new Date(NaN);
}
