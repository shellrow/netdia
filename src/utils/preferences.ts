export type UnitPref = "bits" | "bytes";

export const AUTOSTART_KEY = "np:set:autostart";
export const SIDEBAR_COMPACT_KEY = "np:sidebar:compact";
export const TOOLTIPS_KEY = "np:set:tooltips";
export const THEME_KEY = "np:set:theme";
export const REFRESH_INTERVAL_MS_KEY = "np:set:refresh_ms";
export const BPS_UNIT_KEY = "np:set:bps_unit";

// Local settings
export const LOCAL_SETTINGS = {
  autostart: AUTOSTART_KEY,
  compact:   SIDEBAR_COMPACT_KEY,
  tooltips:  TOOLTIPS_KEY,
  theme:     THEME_KEY,          // system | light | dark
  refresh:   REFRESH_INTERVAL_MS_KEY,
  bpsUnit:   BPS_UNIT_KEY,       //  bits | bytes 
};

export function normalizeBpsUnit(
  value: unknown,
  fallback: UnitPref = "bits",
): UnitPref {
  if (value === "bytes" || value === "bits") return value;
  return fallback;
}

export function readBpsUnit(
  storage: Storage,
  fallback: UnitPref = "bits",
): UnitPref {
  return normalizeBpsUnit(storage.getItem(BPS_UNIT_KEY), fallback);
}
