import { ref, computed } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";

export type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error"
  | "store";

export interface UpdateInfo {
  available: boolean;
  version?: string;
  current_version?: string;
  notes?: string;
  pub_date?: string;
  store_url?: string;
}

export type DownloadEvent =
  | { event: "Started"; data: { content_length: number | null } }
  | {
      event: "Progress";
      data: {
        chunk_length: number;
        downloaded: number;
        content_length: number | null;
      };
    }
  | { event: "Finished"; data?: unknown }
  | { event: "Error"; data: { message: string } };

export function useUpdater() {
  const state = ref<UpdateState>("idle");
  const info = ref<UpdateInfo | null>(null);

  const downloaded = ref(0);
  const total = ref<number | null>(null);
  const error = ref<string | null>(null);

  const isChecking = computed(() => state.value === "checking");
  const isDownloading = computed(() => state.value === "downloading");
  const hasUpdate = computed(() => state.value === "available");
  const storeUrl = computed(() => info.value?.store_url ?? null);

  const uiProgressPercent = computed(() => {
    const t = total.value ?? 0;
    if (t <= 0) return 0;
    const pct = (downloaded.value / t) * 100;
    return Math.max(0, Math.min(100, Math.floor(pct)));
  });

  async function check() {
    state.value = "checking";
    await Promise.resolve();
    error.value = null;

    downloaded.value = 0;
    total.value = null;

    try {
      const res = await invoke<UpdateInfo>("check_update");
      info.value = res;

      if (res.store_url) state.value = "store";
      else if (res.available) state.value = "available";
      else state.value = "idle";
    } catch (e: any) {
      error.value = e?.toString?.() ?? "Failed to check update";
      state.value = "error";
    }
  }

  async function downloadAndInstall() {
    if (!info.value?.available) return;

    state.value = "downloading";
    error.value = null;

    downloaded.value = 0;
    total.value = null;

    const channel = new Channel<DownloadEvent>();
    channel.onmessage = (e) => {
      switch (e.event) {
        case "Started": {
          total.value = e.data.content_length ?? null;
          downloaded.value = 0;
          break;
        }
        case "Progress": {
          downloaded.value = e.data.downloaded ?? 0;
          if (total.value == null && e.data.content_length != null) {
            total.value = e.data.content_length;
          }
          break;
        }
        case "Finished": {
          downloaded.value = total.value ?? downloaded.value;
          state.value = "ready";
          break;
        }
        case "Error": {
          error.value = e.data.message ?? "Update failed";
          state.value = "error";
          break;
        }
      }
    };

    try {
      await invoke("install_update", { onEvent: channel });
    } catch (e: any) {
      error.value = e?.toString?.() ?? "Failed to start update";
      state.value = "error";
    }
  }

  return {
    state,
    info,
    downloaded,
    total,
    error,
    isChecking,
    isDownloading,
    hasUpdate,
    storeUrl,
    uiProgressPercent,
    check,
    downloadAndInstall,
  };
}
