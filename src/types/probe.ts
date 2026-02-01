import { Host } from "./net";

export type ProbeStatusKind = "Done" | "Error" | "Timeout";
export type PingProtocol = "Icmp" | "Tcp" | "Udp" | "Quic" | "Http";

export interface ProbeStatus {
  kind: ProbeStatusKind;
  message: string;
}

export interface PingSample {
  seq: number;
  ip_addr: string;         
  hostname?: string | null;
  port?: number | null;
  rtt_ms?: number | null;
  probe_status: ProbeStatus;
  protocol: PingProtocol;
}

export interface PingStat {
  ip_addr: string;
  hostname?: string | null;
  port?: number | null;
  protocol: PingProtocol;
  samples: PingSample[];
  transmitted_count: number;
  received_count: number;
  min?: number | null;
  avg?: number | null;
  max?: number | null;
}

export interface PingSetting {
  hostname?: string | null;
  ip_addr: string;         
  port?: number | null;
  hop_limit: number;
  protocol: PingProtocol;
  count: number;
  timeout_ms: number;
  send_rate_ms: number;
}

export type PortScanProtocol = "Tcp" | "Quic";
export type TargetPortsPreset = "Common" | "WellKnown" | "Full" | "Top1000" | "Custom";

export type PortState = "Open" | "Closed" | "Filtered";

export interface TlsInfo {
  version?: string | null;
  cipher_suite?: string | null;
  alpn?: string | null;
  sni?: string | null;
  subject?: string | null;
  issuer?: string | null;
  not_before?: string | null;
  not_after?: string | null;
  san_list: string[];
  serial_hex?: string | null;
  sig_algorithm?: string | null;
  pubkey_algorithm?: string | null;
}

export interface ServiceInfo {
  name?: string | null;
  product?: string | null;
  version?: string | null;
  quic_version?: string | null;
  banner?: string | null;
  raw?: string | null;
  cpes: string[];
  tls_info?: TlsInfo | null;
}

export interface PortScanProgress {
  run_id: string;
  done: number;
  total: number;
}

export interface PortScanSample {
  run_id: string;
  ip_addr: string;
  port: number;
  state: PortState;
  rtt_ms?: number | null;
  message?: string | null;
  service_name?: string | null;
  service_info?: ServiceInfo | null;
  done?: number;
  total?: number;
}

export interface PortScanReport {
  run_id: string;
  ip_addr: string;
  hostname?: string | null;
  protocol: PortScanProtocol;
  samples: PortScanSample[];
}

export interface PortScanSetting {
  ip_addr: string;
  hostname?: string | null;
  target_ports_preset: TargetPortsPreset;
  user_ports: number[];
  protocol: PortScanProtocol;
  timeout_ms: number;
  ordered: boolean;
  service_detection: boolean;
}

export type HostState = "Alive" | "Unreachable";

export interface HostScanStartPayload {
  run_id: string;
}

export interface HostScanErrorPayload {
  run_id: string;
  message: string;
}

export interface HostScanCancelledPayload {
  run_id: string;
}

export interface HostScanProgressPayload {
  run_id: string;
  done: number;
  total: number;
}

export interface HostScanProgress {
  run_id: string;
  ip_addr: string;
  state: HostState;
  rtt_ms?: number | null;
  message?: string | null;
  done: number;
  total: number;
}

export interface HostScanReport {
  run_id: string;
  alive: [Host, number][];
  unreachable: Host[];
  total: number;
}

export interface HostScanRequest {
  targets: string[];
  hop_limit: number;
  timeout_ms: number;
  count: number;
  payload?: string | null;
  ordered: boolean;
  concurrency?: number | null;
}

export interface NeighborScanStartPayload {
  run_id: string;
}

export interface NeighborScanCancelledPayload {
  run_id: string;
}

export interface NeighborScanErrorPayload {
  run_id: string;
  message: string;
}

export type NeighborHost = {
  ip_addr: string;
  mac_addr?: string | null;
  vendor?: string | null;
  rtt_ms?: number | null;
  tags: string[];
};

export type NeighborScanReport = {
  run_id: string;
  neighbors: NeighborHost[];
  total: number;
};

export type TraceProtocol = "Icmp" | "Udp";

export interface TraceSetting {
  hostname: string | null;
  ip_addr: string;
  max_hops: number;
  tries_per_hop: number;
  timeout_ms: number;
  protocol: TraceProtocol;
}

export interface TraceHop {
  hop: number;
  ip_addr?: string | null;
  rtt_ms?: number | null;
  reached: boolean;
  note?: string | null;
}

export interface TraceDonePayload {
  reached: boolean;
  hops: number;
  ip_addr: string;
  hostname?: string | null;
  protocol: TraceProtocol;
}
