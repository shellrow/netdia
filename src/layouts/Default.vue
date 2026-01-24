<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { RouteRecordName, useRoute } from "vue-router";
import { useTheme } from "../composables/useTheme";
import { getName as getAppName, getVersion as getAppVersion } from "@tauri-apps/api/app";
import { STORAGE_KEYS } from "../constants/storage";

const innerWidth = ref(window.innerWidth);
const innerHeight = ref(window.innerHeight);
const { currentLogoFile } = useTheme();
const stored = localStorage.getItem(STORAGE_KEYS.SIDEBAR_COMPACT);
const isCompact = ref(stored != null ? stored === "1" : true);
watch(isCompact, v => {
  localStorage.setItem(STORAGE_KEYS.SIDEBAR_COMPACT, v ? "1" : "0");
});
const route = useRoute();
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

const checkWindowSize = () => {
  innerWidth.value = window.innerWidth;
  innerHeight.value = window.innerHeight;
};

onMounted(() => window.addEventListener("resize", checkWindowSize));
onUnmounted(() => window.removeEventListener("resize", checkWindowSize));

type Item = {
  name: string;    
  label: string;
  icon: string;
  aria?: string;
};
const MENU: Item[] = [
  { name: "dashboard",       label: "Dashboard", icon: "pi-chart-bar" },
  { name: "interfaces",      label: "Interfaces", icon: "pi-arrows-h" },
  { name: "monitor",         label: "Monitor", icon: "pi-chart-line" },
  { name: "neighbor",        label: "Neighbor", icon: "pi-refresh" },
  { name: "routes",          label: "Routes", icon: "pi-directions" },
  { name: "socket",          label: "Socket", icon: "pi-link" },
  { name: "internet",        label: "Internet", icon: "pi-globe" },
  { name: "dns",             label: "DNS Lookup", icon: "pi-server" },
  { name: "ping",            label: "Ping", icon: "pi-bolt" },
  { name: "traceroute",      label: "Traceroute", icon: "pi-map" },
  { name: "portscan",        label: "Port Scan", icon: "pi-shield" },
  { name: "hostscan",        label: "Host Scan", icon: "pi-search" },
  { name: "os",       label: "OS", icon: "pi-box", aria: "OS Info" }
];

const aboutVisible = ref(false);
const appName = ref<string>("NetDia");
const appVersion = ref<string>("");

const getMenuNameByRoute = (routeName: RouteRecordName | null | undefined): string => {
  if (typeof routeName !== "string") return "";
  const item = MENU.find(it => it.name === routeName);
  return item?.label ?? "";
};

const currentMenuTitle = computed(() => getMenuNameByRoute(route.name));

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
  <div class="resize-container-8 min-h-screen flex relative lg:static bg-surface-50 dark:bg-surface-950 overflow-hidden">
    <!-- Sidebar -->
    <aside
      id="app-sidebar-1"
      class="bg-surface-0 dark:bg-surface-900 h-screen hidden overflow-y-auto lg:block shrink-0 absolute lg:static left-0 top-0 z-10 border-r border-surface-200 dark:border-surface-700 select-none transition-all duration-300"
      :class="isCompact ? 'w-[50px]' : 'w-[150px]'"
      aria-label="Sidebar navigation"
    >
      <div class="flex flex-col h-full">
        <!-- Brand / Compact toggle -->
        <div class="p-4 flex items-center gap-3" :class="isCompact ? 'justify-center' : ''">
          <!-- <span v-if="!isCompact" class="text-lg font-semibold leading-tight text-surface-900 dark:text-surface-0">NetDia</span> -->
          <img
            v-if="!isCompact"
            :src="currentLogoFile"
            alt="NetDia"
            class="w-9 h-9 select-none"
          />
          <Button
            icon="pi pi-bars"
            text
            class="w-9 h-9 ml-auto"
            :class="isCompact ? 'ml-0!' : ''"
            @click="isCompact = !isCompact"
            v-tooltip.top="isCompact ? 'Expand sidebar' : 'Collapse sidebar'"
            aria-label="Toggle compact sidebar"
            severity="secondary"
          />
        </div>
        <!-- NAV SCROLL (flat) -->
        <nav class="overflow-y-auto flex-1 p-2 flex flex-col gap-1 min-h-0">
          <RouterLink
            v-for="it in MENU"
            :key="it.name"
            :to="{ name: it.name }"
            :class="[
              itemClass(isActive(it.name).value),
              isCompact ? 'justify-center' : ''
            ]"
            :aria-label="it.aria ?? it.label"
            v-tooltip="isCompact ? it.label : undefined"
          >
            <i :class="['pi', it.icon, 'text-surface-500 dark:text-surface-400']" />
            <span v-if="!isCompact" class="font-medium text-sm leading-snug">{{ it.label }}</span>
          </RouterLink>
        </nav>
        <!-- Settings (kept at bottom) -->
        <div class="p-2 mt-auto border-surface-200 dark:border-surface-800 border-t">
          <RouterLink
            :to="{ name: 'settings' }"
            :class="[
              'flex items-center p-2 gap-2 text-surface-900 dark:text-surface-0 rounded-lg hover:bg-surface-50 dark:hover:bg-surface-800 transition-colors duration-150',
              isCompact ? 'justify-center' : ''
            ]"
            aria-label="Settings"
            v-tooltip="isCompact ? 'Settings' : null"
          >
            <i class="pi pi-cog" />
            <span v-if="!isCompact" class="font-medium text-base leading-tight">Settings</span>
          </RouterLink>
        </div>
      </div>
    </aside>
    <!-- Main -->
    <div class="min-h-screen flex flex-col relative flex-auto min-w-0">
      <!-- Topbar -->
      <header class="flex justify-between items-center py-2 px-4 bg-surface-0 dark:bg-surface-900 border-b border-surface-200 dark:border-surface-700 relative lg:static">
        <!-- Mobile sidebar toggle -->
        <div class="flex items-center gap-4">
          <a
            v-styleclass="{
              selector: '#app-sidebar-1',
              enterFromClass: 'hidden',
              enterActiveClass: 'animate-fadeinleft',
              leaveToClass: 'hidden',
              leaveActiveClass: 'animate-fadeoutleft',
              hideOnOutsideClick: true,
              resizeSelector: '.resize-container-8',
              hideOnResize: true
            }"
            class="cursor-pointer flex items-center justify-center lg:hidden text-surface-700 dark:text-surface-100"
            aria-label="Toggle sidebar"
          >
            <i class="pi pi-bars text-xl!" />
          </a>
          <span class="font-semibold leading-tight text-surface-900 dark:text-surface-0">{{ currentMenuTitle }}</span>
        </div>
        <!-- Actions -->
        <div class="flex items-center gap-2">
          <!-- <Button outlined :icon="currentThemeIcon" v-tooltip.bottom="'Toggle Theme'" severity="secondary" class="rounded-lg! w-9 h-9" aria-label="Toggle theme" @click="toggleTheme" /> -->
          <Button outlined icon="pi pi-bell" v-tooltip.bottom="'Notifications'" severity="secondary" class="rounded-lg! w-9 h-9" aria-label="Notifications" />
          <Button outlined icon="pi pi-info-circle" v-tooltip.bottom="'About'" severity="secondary" class="rounded-lg! w-9 h-9" aria-label="About" @click="aboutVisible = true" />
        </div>
      </header>
      <!-- Content -->
      <main class="px-2 pt-2 pb-2 flex flex-col flex-auto min-h-0 overflow-hidden">
        <div class="border border-surface-200 dark:border-surface-700 rounded-2xl bg-surface-0 dark:bg-surface-950 flex-auto min-h-0 overflow-auto p-0">
          <router-view />
        </div>
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
        <span class="font-mono">Cross-platform network diagnostic suite</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Author</span>
        <a
          href="https://github.com/shellrow"
          target="_blank"
          rel="noopener"
          class="text-primary-600 dark:text-primary-400 hover:underline flex items-center gap-1"
        >
          <i class="pi pi-github text-xs"></i>
          Shintaro Ito (shellrow)
        </a>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Repository</span>
        <a
          href="https://github.com/shellrow/netdia"
          target="_blank"
          rel="noopener"
          class="text-primary-600 dark:text-primary-400 hover:underline flex items-center gap-1"
        >
          <i class="pi pi-github text-xs"></i>
          github.com/shellrow/netdia
        </a>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-surface-500 w-24">Support</span>
        <a
          href="https://ko-fi.com/shellrow"
          target="_blank"
          rel="noopener"
          class="text-primary-600 dark:text-primary-400 hover:underline flex items-center gap-1"
        >
          <i class="pi pi-heart text-xs text-red-500"></i>
          Buy me a coffee on Ko-fi
        </a>
      </div>
      <div class="pt-2 text-xs text-surface-500">
        Copyright © {{ new Date().getFullYear() }} Shintaro Ito (shellrow).
      </div>
    </div>
    <template #footer>
      <Button label="Close" text severity="secondary" @click="aboutVisible = false" />
    </template>
  </Dialog>
</template>
