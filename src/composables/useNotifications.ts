import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { AppNotification } from "../types/notification";
import type { UpdateInfo } from "../types/update";
import { loadAppConfig, useAppConfig } from "./useAppConfig";

const notifications = ref<AppNotification[]>([]);
const loading = ref(false);
const saving = ref(false);
const initialized = ref(false);

let loadPromise: Promise<AppNotification[]> | null = null;
let startupPromise: Promise<void> | null = null;

function applyNotifications(next: AppNotification[]) {
  notifications.value = next;
  initialized.value = true;
}

export async function loadNotifications(force = false): Promise<AppNotification[]> {
  if (!force && loadPromise) {
    return loadPromise;
  }
  if (!force && initialized.value) {
    return notifications.value;
  }

  loadPromise = (async () => {
    loading.value = true;
    try {
      const next = await invoke<AppNotification[]>("list_notifications");
      applyNotifications(next);
      return next;
    } finally {
      loading.value = false;
      loadPromise = null;
    }
  })();

  return loadPromise;
}

export async function dismissNotification(id: number): Promise<AppNotification[]> {
  saving.value = true;
  try {
    const next = await invoke<AppNotification[]>("dismiss_notification", { id });
    applyNotifications(next);
    return next;
  } finally {
    saving.value = false;
  }
}

export async function markAllNotificationsRead(): Promise<AppNotification[]> {
  if (!notifications.value.some((notification) => !notification.is_read)) {
    return notifications.value;
  }

  saving.value = true;
  try {
    const next = await invoke<AppNotification[]>("mark_all_notifications_read");
    applyNotifications(next);
    return next;
  } finally {
    saving.value = false;
  }
}

export async function upsertUpdateNotification(info: UpdateInfo): Promise<AppNotification[]> {
  const next = await invoke<AppNotification[]>("upsert_update_notification", {
    payload: {
      version: info.version ?? null,
      current_version: info.current_version ?? null,
      notes: info.notes ?? null,
      pub_date: info.pub_date ?? null,
      store_url: info.store_url ?? null,
    },
  });
  applyNotifications(next);
  return next;
}

export async function initializeNotificationsOnStartup(): Promise<void> {
  if (startupPromise) {
    return startupPromise;
  }

  startupPromise = (async () => {
    try {
      await loadNotifications();
      await loadAppConfig();
      const { config } = useAppConfig();

      if (!config.value?.auto_update_check) {
        return;
      }

      const info = await invoke<UpdateInfo>("check_update");
      if (info.available) {
        await upsertUpdateNotification(info);
      }
    } catch (error) {
      console.warn("Failed to initialize notifications:", error);
    } finally {
      startupPromise = null;
    }
  })();

  return startupPromise;
}

export function useNotifications() {
  const unreadCount = computed(
    () => notifications.value.filter((notification) => !notification.is_read).length,
  );

  return {
    notifications,
    loading,
    saving,
    unreadCount,
    hasUnread: computed(() => unreadCount.value > 0),
    loadNotifications,
    dismissNotification,
    markAllNotificationsRead,
    upsertUpdateNotification,
    initializeNotificationsOnStartup,
  };
}
