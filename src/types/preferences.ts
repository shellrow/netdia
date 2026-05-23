export interface UiPreferences {
  sidebar_compact: boolean;
  last_dns_query: string;
  public_ip_visible: boolean;
  hostname_visible: boolean;
}

export interface UiPreferencesPatch {
  sidebar_compact?: boolean;
  last_dns_query?: string;
  public_ip_visible?: boolean;
  hostname_visible?: boolean;
}
