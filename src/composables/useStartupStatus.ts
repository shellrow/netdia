import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

type StartupStatus = { error: string | null; data_directory: string | null };
const status = ref<StartupStatus>({ error: null, data_directory: null });
const restarting = ref(false);
const retryError = ref<string | null>(null);

export async function loadStartupStatus() {
  status.value = await invoke<StartupStatus>("get_startup_status");
}

async function retryStartup() {
  if (restarting.value) return;
  restarting.value = true;
  retryError.value = null;
  try {
    await invoke("retry_startup");
  } catch (error) {
    retryError.value = String(error);
  } finally {
    restarting.value = false;
  }
}

export function useStartupStatus() {
  return {
    startupError: computed(() => status.value.error),
    dataDirectory: computed(() => status.value.data_directory),
    restarting,
    retryError,
    retryStartup,
  };
}
