// RouteFamily (Rust: enum { Ipv4, Ipv6 })
export type RouteFamily = "Ipv4" | "Ipv6" | "All";

// Destination (Rust: struct RouteDestination { ip: IpAddr, prefix: u8 })
export type RouteDestination = {
  addr: string;        // "192.168.10.0" | "0.0.0.0" | "fe80::"
  prefix_len: number;    // 0..=128
};

// RouteFlag (Rust: enum)
export type RouteFlag =
  | "Up"
  | "Gateway"
  | "Host"
  | "Link"
  | "Reject"
  | "Static"
  | "Loopback"
  | { Other: string };


// Returns a single-character abbreviation (like "U", "G", "H", "L") for a RouteFlag.
export function flagShort(flag: RouteFlag): string {
  if (typeof flag === "object") {
    return flag.Other.charAt(0).toUpperCase();
  }

  switch (flag) {
    case "Up": return "U";
    case "Gateway": return "G";
    case "Host": return "H";
    case "Link": return "L";
    case "Reject": return "R";
    case "Static": return "S";
    case "Loopback": return "L";
    default: return "?";
  }
}

// Returns a human-readable description for a RouteFlag.
export function flagDescription(flag: RouteFlag): string {
  if (typeof flag === "object") {
    return `Other (${flag.Other})`;
  }

  switch (flag) {
    case "Up": return "Up (route is usable)";
    case "Gateway": return "Gateway (next-hop via router)";
    case "Host": return "Host (single-host route)";
    case "Link": return "Link (restricted to local link)";
    case "Reject": return "Reject (deny matching traffic)";
    case "Static": return "Static (manually installed)";
    case "Loopback": return "Loopback (local loopback route)";
    default: return "Unknown flag";
  }
}

// Parses a short abbreviation (e.g. "U", "G") into a RouteFlag.
export function flagFromShort(ch: string): RouteFlag {
  const c = ch.toUpperCase();
  switch (c) {
    case "U": return "Up";
    case "G": return "Gateway";
    case "H": return "Host";
    case "L": return "Link";
    case "R": return "Reject";
    case "S": return "Static";
    default: return { Other: ch };
  }
}

export type RouteProtocol =
  | "Unspecified"
  | "IcmpRedirect"
  | "Kernel"
  | "Boot"
  | "Static"
  | "Gated"
  | "RouterAdvertisement"
  | "Mrt"
  | "Mrouted"
  | "Zebra"
  | "Bird"
  | "DnRouted"
  | "Xorp"
  | "Ntk"
  | "Dhcp"
  | "KeepAlived"
  | "Babel"
  | "Bgp"
  | "Isis"
  | "Ospf"
  | "Rip"
  | "Eigrp"
  | "Local"
  | "NetMgmt"
  | "Icmp"
  | "Egp"
  | "Ggp"
  | "Hello"
  | "Esis"
  | "Cisco"
  | "Bbn"
  | "Autostatic"
  | "StaticNonDod"
  | "Rpl"
  | { other: string };

// RouteScope (Rust: enum | Other(u8))
export type RouteScope =
  | "Global"
  | "Site"
  | "Link"
  | "Host"
  | "Nowhere"
  | { other: number };

// RouteEntry (Rust: struct RouteEntry)
export type RouteEntry = {
  family: RouteFamily;
  destination: RouteDestination;
  gateway?: string | null;
  on_link: boolean;
  ifindex?: number | null;
  ifname?: string | null;
  metric?: number | null;
  flags: RouteFlag[];
  protocol?: RouteProtocol | null;
  scope?: RouteScope | null;
  table?: number | null;
  lifetime_ms?: number | null;
};
