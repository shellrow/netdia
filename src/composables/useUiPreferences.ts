import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { STORAGE_KEYS } from "../constants/storage";
import type { UiPreferences, UiPreferencesPatch } from "../types/preferences";

const DEFAULT_UI_PREFERENCES: UiPreferences = {
  sidebar_compact: true,
  last_dns_query: "example.com",
  public_ip_visible: true,
  hostname_visible: true,
};

const uiPreferences = ref<UiPreferences>({ ...DEFAULT_UI_PREFERENCES });
const loading = ref(false);
const saving = ref(false);
const loaded = ref(false);

let loadPromise: Promise<UiPreferences> | null = null;

function applyUiPreferences(next: UiPreferences) {
  uiPreferences.value = { ...next };
  loaded.value = true;
}

export async function loadUiPreferences(force = false): Promise<UiPreferences> {
  if (!force && loadPromise) {
    return loadPromise;
  }
  if (!force && loaded.value) {
    return uiPreferences.value;
  }

  loadPromise = (async () => {
    loading.value = true;
    try {
      const prefs = await invoke<UiPreferences>("get_ui_preferences");
      applyUiPreferences(prefs);
      return prefs;
    } finally {
      loading.value = false;
      loadPromise = null;
    }
  })();

  return loadPromise;
}

export async function patchUiPreferences(
  patch: UiPreferencesPatch,
): Promise<UiPreferences> {
  if (!Object.keys(patch).length) {
    return uiPreferences.value;
  }
  if (!loaded.value) {
    await loadUiPreferences();
  }

  saving.value = true;
  try {
    const next = await invoke<UiPreferences>("patch_ui_preferences", { patch });
    applyUiPreferences(next);
    return next;
  } finally {
    saving.value = false;
  }
}

export async function migrateLegacyUiPreferences(): Promise<void> {
  const patch: UiPreferencesPatch = {};
  const cleanupKeys: string[] = [];

  const compact = localStorage.getItem(STORAGE_KEYS.SIDEBAR_COMPACT);
  if (compact != null) {
    patch.sidebar_compact = compact === "1";
    cleanupKeys.push(STORAGE_KEYS.SIDEBAR_COMPACT);
  }

  const lastDnsQuery = localStorage.getItem(STORAGE_KEYS.LAST_DNS_QUERY);
  if (lastDnsQuery != null) {
    patch.last_dns_query = lastDnsQuery;
    cleanupKeys.push(STORAGE_KEYS.LAST_DNS_QUERY);
  }

  const publicIpVisible = localStorage.getItem(STORAGE_KEYS.PUBLIC_IP_VISIBLE);
  if (publicIpVisible != null) {
    patch.public_ip_visible = publicIpVisible === "1";
    cleanupKeys.push(STORAGE_KEYS.PUBLIC_IP_VISIBLE);
  }

  const hostnameVisible = localStorage.getItem(STORAGE_KEYS.HOSTNAME_VISIBLE);
  if (hostnameVisible != null) {
    patch.hostname_visible = hostnameVisible === "1";
    cleanupKeys.push(STORAGE_KEYS.HOSTNAME_VISIBLE);
  }

  if (!cleanupKeys.length) {
    return;
  }

  await patchUiPreferences(patch);
  for (const key of cleanupKeys) {
    localStorage.removeItem(key);
  }
}

export function useUiPreferences() {
  return {
    uiPreferences,
    loading,
    saving,
    sidebarCompact: computed(() => uiPreferences.value.sidebar_compact),
    lastDnsQuery: computed(() => uiPreferences.value.last_dns_query),
    publicIpVisible: computed(() => uiPreferences.value.public_ip_visible),
    hostnameVisible: computed(() => uiPreferences.value.hostname_visible),
    loadUiPreferences,
    patchUiPreferences,
    migrateLegacyUiPreferences,
  };
}
