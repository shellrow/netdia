import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { AppConfig } from "../types/config";
import { initTheme } from "./useTheme";

const config = ref<AppConfig | null>(null);
const loading = ref(false);
const saving = ref(false);

let loadPromise: Promise<AppConfig> | null = null;

function applyThemeFromConfig(cfg: AppConfig) {
  initTheme(cfg.theme);
}

export async function loadAppConfig(force = false): Promise<AppConfig> {
  if (!force && config.value) {
    applyThemeFromConfig(config.value);
    return config.value;
  }
  if (!force && loadPromise) {
    return loadPromise;
  }

  loadPromise = (async () => {
    loading.value = true;
    try {
      const cfg = await invoke<AppConfig>("get_config");
      config.value = cfg;
      applyThemeFromConfig(cfg);
      return cfg;
    } finally {
      loading.value = false;
      loadPromise = null;
    }
  })();

  return loadPromise;
}

export async function saveAppConfig(next: AppConfig): Promise<void> {
  saving.value = true;
  try {
    await invoke("save_config", { cfg: next });
    config.value = next;
    applyThemeFromConfig(next);
  } finally {
    saving.value = false;
  }
}

export async function patchAppConfig(
  patch: Partial<AppConfig>,
): Promise<AppConfig> {
  const current = config.value ?? (await loadAppConfig());
  const next: AppConfig = {
    ...current,
    ...patch,
    logging: patch.logging ?? current.logging,
  };
  await saveAppConfig(next);
  return next;
}

export function useAppConfig() {
  return {
    config,
    loading,
    saving,
    theme: computed(() => config.value?.theme ?? "dark"),
    refreshIntervalMs: computed(() => config.value?.refresh_interval_ms ?? 1000),
    bpsUnit: computed(() => config.value?.data_unit ?? "bits"),
    autoInternetCheck: computed(() => config.value?.auto_internet_check ?? true),
    autoInternetCheckIntervalS: computed(
      () => config.value?.auto_internet_check_interval_s ?? 60,
    ),
    autoUpdateCheck: computed(() => config.value?.auto_update_check ?? true),
    loadAppConfig,
    saveAppConfig,
    patchAppConfig,
  };
}
