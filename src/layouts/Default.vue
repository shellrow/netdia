<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { RouteRecordName, useRoute } from "vue-router";
import { useTheme } from "../composables/useTheme";
import { getName as getAppName, getVersion as getAppVersion } from "@tauri-apps/api/app";
import { useUiPreferences } from "../composables/useUiPreferences";
import { useNotifications } from "../composables/useNotifications";
import NotificationDrawer from "../components/NotificationDrawer.vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useStartupStatus } from "../composables/useStartupStatus";
import { useAppStatus } from "../composables/useAppStatus";

const { startupError, dataDirectory, restarting, retryError, retryStartup } = useStartupStatus();
const { currentLogoFile } = useTheme();
const { sidebarCompact, patchUiPreferences } = useUiPreferences();
const { unreadCount, loadNotifications, markAllNotificationsRead } = useNotifications();
const { errorMessage, clearAppError, reportAppError } = useAppStatus();
const isCompact = ref(sidebarCompact.value);
const notificationsVisible = ref(false);
const mobileSidebarOpen = ref(false);
const route = useRoute();
watch(sidebarCompact, (value) => {
  if (isCompact.value !== value) {
    isCompact.value = value;
  }
});
watch(isCompact, (value) => {
  if (!startupError.value && value !== sidebarCompact.value) {
    void patchUiPreferences({ sidebar_compact: value });
  }
});
watch(
  () => route.fullPath,
  () => {
    mobileSidebarOpen.value = false;
  },
);
const isActive = (name: string) => computed(() => route.name === name);

const baseItem =
  "flex items-center cursor-pointer p-3 gap-2 rounded-lg border border-transparent transition-colors duration-150";
const idleColor =
  "text-surface-700 dark:text-surface-200 hover:bg-surface-50 dark:hover:bg-surface-800 hover:border-surface-100 dark:hover:border-surface-700 hover:text-surface-900 dark:hover:text-surface-50";
const activeColor =
  "bg-surface-50 dark:bg-surface-800 text-surface-900 dark:text-surface-50 border-surface-200 dark:border-surface-700";

function itemClass(active: boolean) {
  return `${baseItem} ${active ? activeColor : idleColor}`;
}

type Item = {
  name: string;
  label: string;
  icon: string;
  group: "Observe" | "Diagnose" | "System";
  aria?: string;
};
const MENU: Item[] = [
  { name: "dashboard", label: "Dashboard", icon: "pi-chart-bar", group: "Observe" },
  { name: "interfaces", label: "Interfaces", icon: "pi-arrows-h", group: "Observe" },
  { name: "monitor", label: "Traffic Monitor", icon: "pi-chart-line", group: "Observe" },
  { name: "neighbor", label: "Neighbors", icon: "pi-refresh", group: "Observe" },
  { name: "routes", label: "Routes", icon: "pi-directions", group: "Observe" },
  { name: "socket", label: "Sockets", icon: "pi-link", group: "Observe" },
  { name: "internet", label: "Internet", icon: "pi-globe", group: "Diagnose" },
  { name: "dns", label: "DNS Lookup", icon: "pi-server", group: "Diagnose" },
  { name: "ping", label: "Ping", icon: "pi-bolt", group: "Diagnose" },
  { name: "traceroute", label: "Traceroute", icon: "pi-map", group: "Diagnose" },
  { name: "portscan", label: "Port Scan", icon: "pi-shield", group: "Diagnose" },
  { name: "hostscan", label: "Host Scan", icon: "pi-search", group: "Diagnose" },
  { name: "os", label: "OS Info", icon: "pi-box", group: "System" },
];
const MENU_GROUPS = ["Observe", "Diagnose", "System"] as const;

const aboutVisible = ref(false);
const appName = ref<string>("NetDia");
const appVersion = ref<string>("");

const getMenuNameByRoute = (routeName: RouteRecordName | null | undefined): string => {
  if (typeof routeName !== "string") return "";
  if (routeName === "settings") return "Settings";
  const item = MENU.find(it => it.name === routeName);
  return item?.label ?? "";
};

const currentMenuTitle = computed(() => getMenuNameByRoute(route.name));
const unreadBadgeText = computed(() => (unreadCount.value > 9 ? "9+" : `${unreadCount.value}`));

watch(
  currentMenuTitle,
  (title) => {
    document.title = title ? `${title} · NetDia` : "NetDia";
  },
  { immediate: true },
);

async function openExternal(url: string) {
  try {
    await openUrl(url);
  } catch (error) {
    reportAppError(error, "Failed to open link");
  }
}

watch(notificationsVisible, (visible) => {
  if (visible) {
    void (async () => {
      await loadNotifications();
      await markAllNotificationsRead();
    })();
  }
});

onMounted(async () => {
  try {
    appName.value = await getAppName();
    appVersion.value = await getAppVersion();
  } catch {
    // ignore
  }
});

</script>

<template>
  <a class="skip-link" href="#main-content">Skip to main content</a>
  <div class="resize-container-8 min-h-screen flex relative lg:static bg-surface-50 dark:bg-surface-950 overflow-hidden">
    <button
      v-if="mobileSidebarOpen"
      type="button"
      class="fixed inset-0 z-10 bg-black/30 lg:hidden"
      aria-label="Close sidebar"
      @click="mobileSidebarOpen = false"
    />
    <!-- Sidebar -->
    <aside
      id="app-sidebar-1"
      class="nd-sidebar bg-surface-0 dark:bg-surface-900 h-screen overflow-x-hidden overflow-y-auto shrink-0 absolute lg:static left-0 top-0 z-20 border-r border-surface-200 dark:border-surface-800 select-none transition-all duration-300"
      :class="[
        isCompact ? 'w-16' : 'w-52',
        mobileSidebarOpen ? 'block' : 'hidden lg:block',
      ]"
      aria-label="Sidebar navigation"
    >
      <div class="flex flex-col h-full">
        <!-- Brand / Compact toggle -->
        <div class="h-16 px-3 flex items-center gap-2 border-b border-surface-100 dark:border-surface-800" :class="isCompact ? 'justify-center' : ''">
          <img
            v-if="!isCompact"
            :src="currentLogoFile"
            alt=""
            class="w-8 h-8 select-none shrink-0"
          />
          <span v-if="!isCompact" class="text-base font-bold tracking-tight text-surface-950 dark:text-surface-0">NetDia</span>
          <Button
            :icon="isCompact ? 'pi pi-angle-double-right' : 'pi pi-angle-double-left'"
            text
            class="icon-btn ml-auto"
            :class="isCompact ? 'ml-0!' : ''"
            @click="isCompact = !isCompact"
            v-tooltip.top="isCompact ? 'Expand sidebar' : 'Collapse sidebar'"
            aria-label="Toggle compact sidebar"
            severity="secondary"
          />
        </div>
        <!-- NAV SCROLL (flat) -->
        <nav class="overflow-x-hidden overflow-y-auto flex-1 px-2 py-3 flex flex-col min-h-0">
          <section v-for="group in MENU_GROUPS" :key="group" class="mb-3">
            <div
              v-if="!isCompact"
              class="px-3 pb-1.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-surface-400 dark:text-surface-500"
            >
              {{ group }}
            </div>
            <div class="flex flex-col gap-1">
              <RouterLink
                v-for="it in MENU.filter((item) => item.group === group)"
                :key="it.name"
                :to="{ name: it.name }"
                :class="[
                  itemClass(isActive(it.name).value),
                  isCompact ? 'justify-center' : ''
                ]"
                :aria-label="it.aria ?? it.label"
                :aria-current="isActive(it.name).value ? 'page' : undefined"
                v-tooltip="isCompact ? it.label : undefined"
              >
                <i :class="['pi', it.icon, 'nd-nav-icon']" />
                <span v-if="!isCompact" class="font-medium text-[13px] leading-snug">{{ it.label }}</span>
              </RouterLink>
            </div>
          </section>
        </nav>
        <!-- Settings (kept at bottom) -->
        <div class="p-2 mt-auto border-surface-200 dark:border-surface-800 border-t">
          <RouterLink
            :to="{ name: 'settings' }"
            :class="[
              itemClass(route.name === 'settings'),
              isCompact ? 'justify-center' : ''
            ]"
            aria-label="Settings"
            :aria-current="route.name === 'settings' ? 'page' : undefined"
            v-tooltip="isCompact ? 'Settings' : null"
          >
            <i class="pi pi-cog" />
            <span v-if="!isCompact" class="font-medium text-[13px] leading-tight">Settings</span>
          </RouterLink>
        </div>
      </div>
    </aside>
    <!-- Main -->
    <div class="min-h-screen flex flex-col relative flex-auto min-w-0">
      <!-- Topbar -->
      <header class="nd-topbar flex justify-between items-center min-h-16 py-2 px-4 lg:px-5 bg-surface-0 dark:bg-surface-900 border-b border-surface-200 dark:border-surface-800 relative lg:static">
        <!-- Mobile sidebar toggle -->
        <div class="flex items-center gap-4">
          <button
            type="button"
            class="cursor-pointer flex items-center justify-center lg:hidden text-surface-700 dark:text-surface-100"
            aria-label="Toggle sidebar"
            aria-controls="app-sidebar-1"
            :aria-expanded="mobileSidebarOpen"
            @click="mobileSidebarOpen = !mobileSidebarOpen"
          >
            <i class="pi pi-bars text-xl!" />
          </button>
          <div class="min-w-0">
            <h1 class="text-base font-semibold leading-tight text-surface-950 dark:text-surface-0">{{ currentMenuTitle }}</h1>
          </div>
        </div>
        <!-- Actions -->
        <div class="flex items-center gap-2">
          <!-- <Button outlined :icon="currentThemeIcon" v-tooltip.bottom="'Toggle Theme'" severity="secondary" class="icon-btn" aria-label="Toggle theme" @click="toggleTheme" /> -->
          <div class="relative">
            <Button
              outlined
              icon="pi pi-bell"
              v-tooltip.bottom="'Notifications'"
              severity="secondary"
              class="icon-btn"
              aria-label="Notifications"
              :disabled="!!startupError"
              @click="notificationsVisible = true"
            />
            <span
              v-if="unreadCount > 0"
              class="absolute -right-1 -top-1 inline-flex min-h-5 min-w-5 items-center justify-center rounded-full bg-primary-500 px-1 text-[10px] font-semibold text-white shadow-sm"
            >
              {{ unreadBadgeText }}
            </span>
          </div>
          <Button outlined icon="pi pi-info-circle" v-tooltip.bottom="'About'" severity="secondary" class="icon-btn" aria-label="About" @click="aboutVisible = true" />
        </div>
      </header>
      <div
        v-if="errorMessage"
        class="nd-status-banner mx-3 mt-3 lg:mx-5 flex items-start gap-3 rounded-xl border border-red-200 bg-red-50 px-3 py-2.5 text-sm text-red-800 dark:border-red-900/70 dark:bg-red-950/40 dark:text-red-200"
        role="alert"
        aria-live="assertive"
      >
        <i class="pi pi-exclamation-triangle mt-0.5" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <div class="font-semibold">Some data could not be loaded</div>
          <div class="mt-0.5 break-words text-xs opacity-90">{{ errorMessage }}</div>
        </div>
        <Button
          text
          icon="pi pi-times"
          severity="danger"
          class="icon-btn"
          aria-label="Dismiss error"
          @click="clearAppError"
        />
      </div>
      <div
        v-if="startupError"
        role="alert"
        class="mx-3 mt-3 lg:mx-5 shrink-0 rounded-xl border border-red-300 bg-red-50 p-3 text-red-900 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200"
      >
        <p class="font-semibold">Saved data is unavailable</p>
        <p class="mt-1 text-sm">Diagnostics remain available. Saving settings and preferences is disabled until startup succeeds.</p>
        <p class="mt-1 break-words text-xs">{{ startupError }}</p>
        <p v-if="dataDirectory" class="mt-1 break-words text-xs">Data folder: {{ dataDirectory }}</p>
        <p class="mt-1 text-xs">Close other NetDia instances and retry. If the error persists, quit NetDia and back up the entire data folder before restoring a compatible backup or using a newer version. No database reset is performed.</p>
        <Button class="mt-2" label="Restart and retry" :loading="restarting" @click="retryStartup" />
        <p v-if="retryError" class="mt-1 text-xs">{{ retryError }}</p>
      </div>
      <!-- Content -->
      <main
        id="main-content"
        tabindex="-1"
        class="px-0 pt-0 pb-0 flex flex-col flex-auto min-h-0"
        :class="startupError ? 'overflow-y-auto' : 'overflow-hidden'"
      >
        <router-view />
      </main>
    </div>
  </div>
  <!-- About Dialog -->
  <Dialog
    v-model:visible="aboutVisible"
    modal
    appendTo="body"
    :draggable="false"
    :style="{ width: '32rem', maxWidth: '95vw' }"
  >
    <template #header>
      <div class="flex items-center gap-2">
        <i class="pi pi-info-circle text-surface-500"></i>
        <span class="font-semibold">About</span>
      </div>
    </template>
    <div class="text-sm space-y-3">
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Application</span>
        <span class="font-medium truncate">{{ appName }}</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Version</span>
        <span class="font-mono">{{ appVersion || '-' }}</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Description</span>
        <span class="font-mono">Cross-platform network diagnostic tool</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Author</span>
        <button
          type="button"
          class="text-primary-600 dark:text-primary-400 hover:underline flex items-center gap-1 text-left"
          @click="openExternal('https://github.com/shellrow')"
        >
          <i class="pi pi-github text-xs"></i>
          Shintaro Ito (shellrow)
        </button>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Repository</span>
        <button
          type="button"
          class="text-primary-600 dark:text-primary-400 hover:underline flex items-center gap-1 text-left"
          @click="openExternal('https://github.com/shellrow/netdia')"
        >
          <i class="pi pi-github text-xs"></i>
          github.com/shellrow/netdia
        </button>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Support</span>
        <button
          type="button"
          class="text-primary-600 dark:text-primary-400 hover:underline flex items-center gap-1 text-left"
          @click="openExternal('https://donate.stripe.com/3cIbJ046Q0671z6ctHgIo01')"
        >
          <i class="pi pi-heart text-xs text-red-500"></i>
          Support via Stripe
        </button>
      </div>
      <div class="pt-2 text-xs text-surface-500">
        Copyright © {{ new Date().getFullYear() }} Shintaro Ito (shellrow).
      </div>
    </div>
    <template #footer>
      <Button label="Close" text severity="secondary" @click="aboutVisible = false" />
    </template>
  </Dialog>
  <NotificationDrawer v-model:visible="notificationsVisible" />
</template>
