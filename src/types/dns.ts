export type MxRecord = { preference: number; exchange: string };
export type SoaRecord = {
  mname: string; rname: string; serial: number;
  refresh: number; retry: number; expire: number; minimum: number;
};
export type SrvRecord = { priority: number; weight: number; port: number; target: string };
export type TlsaRecord = { cert_usage: number; selector: number; matching: number; cert_data_base64: string };
export type TxtRecord = { key: string; value: string };
export type CertRecord = { cert_type: number; key_tag: number; algorithm: number; cert_data_base64: string };

export type DomainLookupInfo = {
  name: string;
  a: string[];
  aaaa: string[];
  mx: MxRecord[];
  ns: string[];
  soa: SoaRecord[];
  srv: SrvRecord[];
  tlsa: TlsaRecord[];
  txt: TxtRecord[];
  cert: CertRecord[];
};
