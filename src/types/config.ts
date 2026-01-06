export interface LoggingConfig {
  level: "DEBUG" | "INFO" | "WARN" | "ERROR";
  file_path?: string | null;
}

export interface AppConfig {
  startup: boolean;
  refresh_interval_ms: number;
  theme: "system" | "light" | "dark";
  data_unit: "bits" | "bytes";
  logging: LoggingConfig;
}
