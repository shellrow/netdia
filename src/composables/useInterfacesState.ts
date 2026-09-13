import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createListenerScope } from "../utils/listenerScope";
import type { NetworkInterface } from "../types/net";

const interfaces = ref<NetworkInterface[]>([]);
const loading = ref(false);
const ready = ref(false);
const statsRevision = ref(0);
const interfacesRevision = ref(0);
const error = ref<string | null>(null);

let startPromise: Promise<void> | null = null;
let debouncing = false;

async function fetchInterfaces(withLoading = false) {
  if (withLoading) {
    loading.value = true;
  }
  try {
    const data = await invoke<NetworkInterface[]>("get_network_interfaces");
    interfaces.value = data ?? [];
    error.value = null;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    throw cause;
  } finally {
    if (withLoading) {
      loading.value = false;
    }
  }
}

async function handleStatsUpdated() {
  if (debouncing) return;
  debouncing = true;
  window.setTimeout(async () => {
    try {
      await fetchInterfaces(false);
      statsRevision.value += 1;
    } catch {
      // Keep the shared error state and allow the next event to retry.
    } finally {
      debouncing = false;
    }
  }, 500);
}

async function handleInterfacesUpdated() {
  loading.value = true;
  try {
    await fetchInterfaces(false);
    interfacesRevision.value += 1;
  } catch {
    // fetchInterfaces already exposes the error; event callbacks must not reject.
  } finally {
    loading.value = false;
  }
}

export async function ensureInterfacesState(): Promise<void> {
  if (ready.value) return;
  if (startPromise) return startPromise;

  startPromise = (async () => {
    await fetchInterfaces(true);
    const scope = createListenerScope(listen);
    try {
      await scope.listen("stats_updated", handleStatsUpdated);
      await scope.listen("interfaces_updated", handleInterfacesUpdated);
    } catch (cause) {
      scope.dispose();
      error.value = String(cause);
      throw cause;
    }
    ready.value = true;
  })();

  try {
    await startPromise;
  } finally {
    startPromise = null;
  }
}

export async function reloadSharedInterfaces() {
  loading.value = true;
  try {
    await invoke("reload_interfaces");
    await fetchInterfaces(false);
    interfacesRevision.value += 1;
  } finally {
    loading.value = false;
  }
}

export function useInterfacesState() {
  return {
    interfaces,
    loading,
    ready,
    statsRevision,
    interfacesRevision,
    error,
    ensureInterfacesState,
    reloadSharedInterfaces,
  };
}
