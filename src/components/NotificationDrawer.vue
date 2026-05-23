<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import { useNotifications } from "../composables/useNotifications";
import type { AppNotification } from "../types/notification";

const visible = defineModel<boolean>("visible", { required: true });

const router = useRouter();
const {
  notifications,
  loading,
  dismissNotification,
} = useNotifications();

const sortedNotifications = computed(() => notifications.value);

function iconClass(notification: AppNotification) {
  switch (notification.kind) {
    case "update_available":
      return "pi pi-download";
    default:
      return "pi pi-bell";
  }
}

function releaseDate(notification: AppNotification) {
  const value = notification.data?.pub_date;
  return value ? (value.split("T")[0] ?? value) : null;
}

async function openNotification(notification: AppNotification) {
  if (notification.kind === "update_available") {
    await router.push({ name: "settings", query: { section: "app" } });
    visible.value = false;
  }
}

async function dismiss(id: number) {
  await dismissNotification(id);
}
</script>

<template>
  <Drawer
    v-model:visible="visible"
    position="right"
    :style="{ width: '28rem', maxWidth: '100vw' }"
    header="Notifications"
    modal
  >
    <div class="flex h-full min-h-0 flex-col">

      <div v-if="loading" class="py-8 text-center text-sm text-surface-500">
        Loading notifications...
      </div>

      <div v-else-if="!sortedNotifications.length" class="py-8 text-center text-sm text-surface-500">
        No notifications yet.
      </div>

      <div v-else class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
        <article
          v-for="notification in sortedNotifications"
          :key="notification.id"
          class="rounded-2xl border border-surface-200 bg-surface-50 p-4 transition-colors dark:border-surface-700 dark:bg-surface-900"
          :class="notification.is_read ? 'opacity-80' : ''"
        >
          <div class="flex items-start gap-3">
            <div class="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-primary-50 text-primary-600 dark:bg-primary-900/40 dark:text-primary-300">
              <i :class="iconClass(notification)" />
            </div>

            <div class="min-w-0 flex-1">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <div class="font-medium text-surface-900 dark:text-surface-0">
                    {{ notification.title }}
                  </div>
                  <div class="mt-1 text-sm text-surface-600 dark:text-surface-300">
                    {{ notification.message }}
                  </div>
                </div>

                <Button
                  icon="pi pi-times"
                  text
                  rounded
                  severity="secondary"
                  aria-label="Dismiss notification"
                  @click="dismiss(notification.id)"
                />
              </div>

              <div
                v-if="notification.data?.version || releaseDate(notification)"
                class="mt-3 flex flex-wrap items-center gap-2 text-xs text-surface-500"
              >
                <span v-if="notification.data?.version">Version {{ notification.data.version }}</span>
                <span v-if="releaseDate(notification)">Released: {{ releaseDate(notification) }}</span>
              </div>

              <div
                v-if="notification.data?.notes"
                class="mt-3 whitespace-pre-wrap rounded-xl bg-surface-0 px-3 py-2 text-xs text-surface-600 dark:bg-surface-950 dark:text-surface-300"
              >
                {{ notification.data.notes }}
              </div>

              <div class="mt-4 flex items-center gap-2">
                <Button
                  v-if="notification.kind === 'update_available'"
                  label="Open update settings"
                  icon="pi pi-arrow-right"
                  size="small"
                  @click="openNotification(notification)"
                />
                <Tag
                  v-if="!notification.is_read"
                  severity="info"
                  value="Unread"
                />
              </div>
            </div>
          </div>
        </article>
      </div>
    </div>
  </Drawer>
</template>
