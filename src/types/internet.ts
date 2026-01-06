export type IpInfo = {
  ip_version: string;   // "IPv4" | "IPv6" (string as given by API)
  ip_addr_dec: string;
  ip_addr: string;
  host_name: string;
  network: string;
  asn: string;
  as_name: string;
  country_code: string;
  country_name: string;
};

export type IpInfoDual = {
  ipv4?: IpInfo | null;
  ipv6?: IpInfo | null;
};
