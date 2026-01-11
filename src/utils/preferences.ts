import { STORAGE_KEYS } from "../constants/storage";

export type UnitPref = "bits" | "bytes";

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
  return normalizeBpsUnit(storage.getItem(STORAGE_KEYS.BPS_UNIT), fallback);
}
