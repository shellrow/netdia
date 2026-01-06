export type ProxyEnv = {
  http?: string | null;
  https?: string | null;
  all?: string | null;
  no_proxy?: string | null;
};

export type SysInfo = {
  hostname: string;
  os_type: string;
  os_version: string;
  kernel_version?: string | null;
  edition: string;
  codename: string;
  bitness: string;       // "64-bit" | "32-bit"
  architecture: string;  // "x86_64" | "aarch64" | ...
  proxy: ProxyEnv;
};
