<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { NetworkInterface } from "../types/net";
import { formatBps, formatBytesPerSec, formatBytes } from "../types/net";
import type { SysInfo } from "../types/system";
import { fmtIfType, severityByOper } from "../utils/formatter";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { usePrivacyGate } from "../composables/usePrivacyGate";
import type { ChartData, ChartOptions } from "chart.js";
import { hexToRgba } from "../utils/color";
import { DEFAULT_AUTO_INTERNET_CHECK, INTERNET_CHECK_INTERVAL } from "../constants/defaults";
import { STORAGE_KEYS } from "../constants/storage";
import { readBpsUnit, type UnitPref } from "../utils/preferences";
import { useRouter } from "vue-router";
import type { IpInfoDual } from "../types/internet";
import { clampInt } from "../utils/numeric";

// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();
const { publicIpVisible, togglePublicIp, pubIpGate, hostnameVisible, toggleHostname } =
  usePrivacyGate();

const loading = ref(false);
const ifaces = ref<NetworkInterface[]>([]);
const sys = ref<SysInfo | null>(null);

const bpsUnit = ref<UnitPref>(readBpsUnit(localStorage));

function refreshUnitPref() {
  bpsUnit.value = readBpsUnit(localStorage);
}
const rxLabel = computed(() =>
  bpsUnit.value === "bits" ? "RX bps" : "RX B/s",
);
const txLabel = computed(() =>
  bpsUnit.value === "bits" ? "TX bps" : "TX B/s",
);
function formatThroughput(v?: number): string {
  const n = v ?? 0;
  return bpsUnit.value === "bits" ? formatBps(n * 8) : formatBytesPerSec(n);
}

function maskIpLabel(
  v: string | { addr: string; prefix_len: number },
): string {
  const raw = typeof v === "string" ? v : `${v.addr}/${v.prefix_len}`;
  return pubIpGate(raw);
}

function maskMac(mac?: string | null): string {
  if (!mac) return "-";
  return pubIpGate(mac);
}

const defaultIface = computed<NetworkInterface | null>(() => {
  const d = ifaces.value.find((i) => i.default) ?? null;
  if (d) return d;
  const cand = ifaces.value.find(
    (i) =>
      (i.oper_state ?? "").toLowerCase() === "up" && !!i.gateway,
  );
  return cand ?? null;
});

// Live Traffic Chart
const documentStyle = getComputedStyle(document.documentElement);
const textColor = documentStyle.getPropertyValue('--p-text-color');
const textColorSecondary = documentStyle.getPropertyValue('--p-text-muted-color');
const surfaceBorder = documentStyle.getPropertyValue('--p-content-border-color');
const rxBorder = documentStyle.getPropertyValue("--p-cyan-400").trim();
const txBorder = documentStyle.getPropertyValue("--p-pink-400").trim();

function formatAxisThroughput(value: number): string {
  const bytesPerSec = value ?? 0;
  const useBits = bpsUnit.value === "bits";
  const base = useBits ? bytesPerSec * 8 : bytesPerSec;

  const unitSuffix = useBits ? "bps" : "B/s";

  const abs = Math.abs(base);
  if (abs >= 1_000_000_000) {
    return `${(base / 1_000_000_000).toFixed(1)} G${unitSuffix}`;
  }
  if (abs >= 1_000_000) {
    return `${(base / 1_000_000).toFixed(1)} M${unitSuffix}`;
  }
  if (abs >= 1_000) {
    return `${(base / 1_000).toFixed(1)} K${unitSuffix}`;
  }
  return `${Math.round(base)} ${unitSuffix}`;
}

const trafficData = ref<ChartData<"line">>({
  labels: [],
  datasets: [
    {
      label: "RX",
      data: [],
      borderColor: rxBorder,
      backgroundColor: hexToRgba(rxBorder, 0.15),
      fill: true,
      tension: 0.25,
    },
    {
      label: "TX",
      data: [],
      borderColor: txBorder,
      backgroundColor: hexToRgba(txBorder, 0.15),
      fill: true,
      tension: 0.25,
    },
  ],
});

const trafficOptions = ref<ChartOptions<"line">>({
  responsive: true,
  maintainAspectRatio: false,
  animation: false,
  plugins: {
    legend: {
      display: true,
      position: "bottom",
      labels: {
        color: textColor
      }
    },
    tooltip: {
      callbacks: {
        label(ctx) {
          const dsLabel = ctx.dataset.label || "";
          const raw = ctx.parsed.y ?? 0;
          const v =
            bpsUnit.value === "bits"
              ? formatBps(raw * 8)
              : formatBytesPerSec(raw);
          return `${dsLabel}: ${v}`;
        },
      },
    },
  },
  scales: {
    x: {
      title: { display: false },
      ticks: {
        color: textColorSecondary,
        font: { size: 10 },
        maxTicksLimit: 8,
        maxRotation: 0,
        minRotation: 0,
      },
      grid: {
        color: surfaceBorder
      }
    },
    y: {
      beginAtZero: true,
      grace: '5%',
      suggestedMax: 1_000,
      ticks: {
        callback(value) {
          const num = typeof value === "number" ? value : Number(value);
          return formatAxisThroughput(Number.isFinite(num) ? num : 0);
        },
        color: textColorSecondary,
        font: { size: 10 },
        maxTicksLimit: 6,
      },
      grid: {
        color: surfaceBorder
      }
    },
  },
});

function initTrafficChart() {
  const now = Date.now();
  // Generate last 30 seconds labels as initial labels
  const labels = Array.from({ length: 30 }, (_, i) =>
    new Date(now - (29 - i) * 1000).toLocaleTimeString()
  );
  const zeros = Array(30).fill(0);
  trafficData.value = {
    labels,
    datasets: [
      {
        ...(trafficData.value.datasets?.[0] || {}),
        label: rxLabel.value,
        data: [...zeros],
      },
      {
        ...(trafficData.value.datasets?.[1] || {}),
        label: txLabel.value,
        data: [...zeros],
      },
    ],
  };
}

function refreshTrafficLabels() {
  trafficData.value = {
    ...trafficData.value,
    datasets: [
      {
        ...(trafficData.value.datasets?.[0] || {}),
        label: rxLabel.value,
      },
      {
        ...(trafficData.value.datasets?.[1] || {}),
        label: txLabel.value,
      },
    ],
  };
}

// Traffic sample from default interface
function pushTrafficSample() {
  const iface = defaultIface.value;
  if (!iface || !iface.stats) return;

  const now = new Date();
  const label = now.toLocaleTimeString();
  const rx = iface.stats.rx_bytes_per_sec || 0;
  const tx = iface.stats.tx_bytes_per_sec || 0;

  const current = trafficData.value;
  const labels = [...(current.labels ?? []), label].slice(-30); // Last 30 points only
  const rxData = [
    ...((current.datasets?.[0].data as number[] | undefined) ?? []),
    rx,
  ].slice(-30);
  const txData = [
    ...((current.datasets?.[1].data as number[] | undefined) ?? []),
    tx,
  ].slice(-30);

  trafficData.value = {
    ...current,
    labels,
    datasets: [
      {
        ...(current.datasets?.[0] || {}),
        label: rxLabel.value,
        data: rxData,
      },
      {
        ...(current.datasets?.[1] || {}),
        label: txLabel.value,
        data: txData,
      },
    ],
  };
}

function calcStatsFromDataset(index: number) {
  const ds = trafficData.value.datasets?.[index];
  if (!ds || !ds.data) return null;

  const arr = (ds.data as number[])
    .map(v => Number(v))
    .filter(v => Number.isFinite(v));

  if (arr.length === 0) return null;

  let min = arr[0];
  let max = arr[0];
  let sum = 0;

  for (const v of arr) {
    if (v < min) min = v;
    if (v > max) max = v;
    sum += v;
  }

  const avg = sum / arr.length;
  return { min, avg, max };
}

const rxStats = computed(() => calcStatsFromDataset(0));
const txStats = computed(() => calcStatsFromDataset(1));

function formatStat(v?: number) {
  if (v == null || !Number.isFinite(v)) return "-";
  return formatThroughput(v);
}

// Data Fetching

async function fetchInterfaces() {
  try {
    const data = (await invoke(
      "get_network_interfaces",
    )) as NetworkInterface[];
    ifaces.value = data;
  } finally {
    /* noop */
  }
}

async function fetchSysInfo() {
  try {
    const data = (await invoke("get_sys_info")) as SysInfo;
    sys.value = data;
  } finally {
    /* noop */
  }
}

async function fetchAll() {
  loading.value = true;
  try {
    const [ifs, si] = await Promise.all([
      invoke("get_network_interfaces") as Promise<NetworkInterface[]>,
      invoke("get_sys_info") as Promise<SysInfo>,
    ]);
    ifaces.value = ifs ?? [];
    sys.value = si ?? null;

    refreshTrafficLabels();
    pushTrafficSample();
  } finally {
    loading.value = false;
  }
}

let unlistenStats: UnlistenFn | null = null;
let unlistenIfaces: UnlistenFn | null = null;
let debouncing = false;

async function onStatsUpdated() {
  // Debounce to avoid excessive refreshes when stats are frequent
  if (debouncing) return;
  debouncing = true;
  setTimeout(async () => {
    refreshUnitPref();
    await fetchInterfaces();
    pushTrafficSample();
    debouncing = false;
  }, 500);
}

async function onInterfacesUpdated() {
  loading.value = true;
  try {
    refreshUnitPref();
    await fetchInterfaces();
    await fetchSysInfo();
    // When the IF configuration itself changes, update the sample once
    pushTrafficSample();
  } finally {
    loading.value = false;
  }
}

function togglePrivacy() {
  togglePublicIp();
  toggleHostname();
}

// --- Internet Reachability ---
function readAutoInternetCheckIntervalS(ls: Storage): number {
  const raw = ls.getItem(STORAGE_KEYS.AUTO_INTERNET_CHECK_INTERVAL_S);
  if (raw == null || raw.trim() === "") return INTERNET_CHECK_INTERVAL.DEFAULT;

  const n = Number(raw);
  if (!Number.isFinite(n)) return INTERNET_CHECK_INTERVAL.DEFAULT;

  return clampInt(
    Math.floor(n),
    INTERNET_CHECK_INTERVAL.MIN,
    INTERNET_CHECK_INTERVAL.MAX,
  );
}

function readAutoInternetCheck(ls: Storage): boolean {
  const v = ls.getItem(STORAGE_KEYS.AUTO_INTERNET_CHECK);
  if (v == null) return DEFAULT_AUTO_INTERNET_CHECK;
  return v === "1" || v.toLowerCase() === "true";
}

function writeAutoInternetCheck(ls: Storage, enabled: boolean) {
  ls.setItem(STORAGE_KEYS.AUTO_INTERNET_CHECK, enabled ? "1" : "0");
}

const autoInternetCheck = ref<boolean>(readAutoInternetCheck(localStorage));
const autoInternetCheckIntervalS = ref<number>(
  readAutoInternetCheckIntervalS(localStorage),
);

function refreshAutoInternetCheckIntervalPref() {
  autoInternetCheckIntervalS.value = readAutoInternetCheckIntervalS(localStorage);
}

function refreshAutoInternetCheckPref() {
  autoInternetCheck.value = readAutoInternetCheck(localStorage);
}

function startIpInfoTimer(manual: boolean = false) {
  if (ipInfoTimer) clearInterval(ipInfoTimer);
  if (!autoInternetCheck.value) {
    ipInfoTimer = null;
    return;
  }
  refreshIpInfo(manual);
  const intervalMs = autoInternetCheckIntervalS.value * 1000;
  ipInfoTimer = window.setInterval(() => refreshIpInfo(false), intervalMs);
}

function toggleAutoInternetCheck() {
  const prev = autoInternetCheck.value;
  autoInternetCheck.value = !prev;
  writeAutoInternetCheck(localStorage, autoInternetCheck.value);
  startIpInfoTimer(!prev && autoInternetCheck.value);
}

// --- Public IP / Reachability ---
const ipInfo = ref<IpInfoDual | null>(null);
const ipInfoLoading = ref(false);
const ipInfoError = ref<string | null>(null);
const ipInfoLastOkAt = ref<number | null>(null);

let ipInfoTimer: number | null = null;

async function refreshIpInfo(manual: boolean = false) {
  if (!autoInternetCheck.value && !manual) return;
  if (ipInfoLoading.value && !manual) return;
  ipInfoLoading.value = true;
  try {
    const data = (await invoke("get_public_ip_info")) as IpInfoDual;
    ipInfo.value = data ?? null;
    ipInfoError.value = null;
    ipInfoLastOkAt.value = Date.now();
  } catch (e: any) {
    ipInfoError.value = String(e?.message ?? e);
  } finally {
    ipInfoLoading.value = false;
  }
}

function isReachable(): boolean {
  const v4 = ipInfo.value?.ipv4?.ip_addr;
  const v6 = ipInfo.value?.ipv6?.ip_addr;
  return !!v4 || !!v6;
}

function internetHealth(): Health {
  if (!ipInfoLastOkAt.value && !autoInternetCheck.value) return "unknown"; 
  if (!ipInfoLastOkAt.value && !ipInfoError.value) return "unknown";
  if (ipInfoError.value) return "bad";
  return isReachable() ? "ok" : "warn";
}

function publicIpSubtitle(): string {
  if (!ipInfoLastOkAt.value && !autoInternetCheck.value) return "Auto check is OFF";
  if (!ipInfoLastOkAt.value && !ipInfoError.value) return "Checking...";
  if (ipInfoError.value) return "Check failed";
  if (!publicIpVisible.value) return "Hidden";

  const v4 = ipInfo.value?.ipv4?.ip_addr;
  const v6 = ipInfo.value?.ipv6?.ip_addr;
  if (v4) return pubIpGate(v4);
  if (v6) return pubIpGate(v6);
  return "Not detected";
}

function internetSubtitle(): string {
  const h = internetHealth();
  if (h === "unknown") return "Unknown";
  if (h === "bad") return "Unreachable";
  if (h === "warn") return "No public IP";
  return "Reachable";
}

function publicAsSummary(): string {
  if (!ipInfo.value) return "IPv4 / IPv6";

  const v4 = ipInfo.value.ipv4;
  const v6 = ipInfo.value.ipv6;

  const asName =
    v4?.as_name ||
    v6?.as_name ||
    v4?.asn ||
    v6?.asn ||
    null;

  if (!asName) return "IPv4 / IPv6";

  return pubIpGate(asName);
}

// --- Network Path ---
type Health = "ok" | "warn" | "bad" | "unknown";
type PathNodeType = "device" | "iface" | "gateway" | "dns" | "public" | "internet";

type PathNode = {
  type: PathNodeType;
  title: string;
  subtitle?: string;
  summary?: string;
  icon: string;
  health: Health;
  color: string;
  onClick?: () => void;
};

// --- Path node detail dialog ---
type PathDetailKind = "gateway" | "dns" | null;

const pathDetailOpen = ref(false);
const pathDetailKind = ref<PathDetailKind>(null);

function openPathDetail(kind: Exclude<PathDetailKind, null>) {
  pathDetailKind.value = kind;
  pathDetailOpen.value = true;
}

const gwDetail = computed(() => {
  const iface = defaultIface.value;
  const gw = iface?.gateway;
  if (!gw) return null;
  return {
    mac: gw.mac_addr ?? null,
    ipv4: gw.ipv4 ?? [],
    ipv6: gw.ipv6 ?? [],
  };
});

const dnsDetail = computed(() => {
  const iface = defaultIface.value;
  return {
    servers: iface?.dns_servers ?? [],
  };
});

const router = useRouter();

function healthColor(h: Health) {
  const s = getComputedStyle(document.documentElement);
  switch (h) {
    case "ok": return s.getPropertyValue("--p-green-500").trim() || "var(--green-500)";
    case "warn": return s.getPropertyValue("--p-yellow-500").trim() || "var(--yellow-500)";
    case "bad": return s.getPropertyValue("--p-red-500").trim() || "var(--red-500)";
    default: return s.getPropertyValue("--p-surface-400").trim() || "var(--surface-400)";
  }
}

const pathNodes = computed<PathNode[]>(() => {
  const n: PathNode[] = [];

  // 1. This device
  n.push({
    type: "device",
    title: "This Device",
    subtitle: hostnameVisible.value && sys.value?.hostname ? sys.value.hostname : "Hostname hidden",
    summary: sys.value ? `${sys.value.os_type ?? ""} ${sys.value.architecture ?? ""}`.trim() : "",
    icon: "pi pi-desktop",
    health: sys.value ? "ok" : "unknown",
    color: healthColor(sys.value ? "ok" : "unknown"),
  });

  // 2. Default interface
  const iface = defaultIface.value;
  n.push({
    type: "iface",
    title: "Default IF",
    subtitle: iface ? iface.name : "Not detected",
    summary: iface?.ipv4?.[0]
      ? `IPv4: ${maskIpLabel(iface.ipv4[0])}`
      : (iface ? "No IPv4" : ""),
    icon: "pi pi-sitemap",
    health: iface ? "ok" : "bad",
    color: healthColor(iface ? "ok" : "bad"),
    onClick: () => router.push({ name: "interfaces" }),
  });

  // 3. Gateway
  const gwv4 = iface?.gateway?.ipv4?.[0];
  const gwv6 = iface?.gateway?.ipv6?.[0];
  const gwIp = gwv4 || gwv6 || "";
  n.push({
    type: "gateway",
    title: "Gateway",
    subtitle: gwIp ? pubIpGate(gwIp) : "Not found",
    summary: iface?.gateway?.mac_addr ? `MAC: ${maskMac(iface.gateway.mac_addr)}` : "",
    icon: "pi pi-directions",
    health: gwIp ? "ok" : "warn",
    color: healthColor(gwIp ? "ok" : "warn"),
    onClick: () => openPathDetail("gateway"),
  });

  // 4. DNS
  const dns = iface?.dns_servers ?? [];
  n.push({
    type: "dns",
    title: "DNS",
    subtitle: dns.length ? pubIpGate(dns[0]) : "Not set",
    summary: dns.length > 1 ? `+${dns.length - 1} more` : "",
    icon: "pi pi-server",
    health: dns.length ? "ok" : "warn",
    color: healthColor(dns.length ? "ok" : "warn"),
    onClick: () => openPathDetail("dns"),
  });

  // 5. Public IP
  n.push({
    type: "public",
    title: "Public IP",
    subtitle: publicIpSubtitle(),
    summary: publicAsSummary(),
    icon: "pi pi-globe",
    health: internetHealth(),
    color: healthColor(internetHealth()),
    onClick: () => router.push({ name: "internet" }),
  });

  // 6. Internet reachability
  n.push({
    type: "internet",
    title: "Internet",
    subtitle: internetSubtitle(),
    summary: "",
    icon: "pi pi-wifi",
    health: internetHealth(),
    color: healthColor(internetHealth()),
    onClick: () => router.push({ name: "internet" }),
  });

  return n;
});

function onPathNodeClick(node: PathNode) {
  node.onClick?.();
}

const onStorage = () => {
  refreshUnitPref();
  const prevEnabled = autoInternetCheck.value;
  const prevInterval = autoInternetCheckIntervalS.value;

  refreshAutoInternetCheckPref();
  refreshAutoInternetCheckIntervalPref();

  if (autoInternetCheck.value) {
    const enabledChanged = prevEnabled !== autoInternetCheck.value;
    const intervalChanged = prevInterval !== autoInternetCheckIntervalS.value;
    if (enabledChanged || intervalChanged) startIpInfoTimer(false);
  } else {
    if (ipInfoTimer) {
      clearInterval(ipInfoTimer);
      ipInfoTimer = null;
    }
  }
};

onMounted(async () => {
  initTrafficChart();
  refreshUnitPref();
  await fetchAll();

  unlistenStats = await listen("stats_updated", onStatsUpdated);
  unlistenIfaces = await listen("interfaces_updated", onInterfacesUpdated);
  window.addEventListener("storage", onStorage);

  startIpInfoTimer();
});

onBeforeUnmount(() => {
  unlistenStats?.();
  unlistenIfaces?.();
  window.removeEventListener("storage", onStorage);

  if (ipInfoTimer) {
    clearInterval(ipInfoTimer);
    ipInfoTimer = null;
  }

});
</script>

<style scoped>
.nd-marker {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: white;
  border-radius: 9999px;
  box-shadow: var(--shadow-2);
  z-index: 10;
}

.nd-node-card {
  cursor: pointer;
  user-select: none;
  transition: transform 0.08s ease, box-shadow 0.08s ease;
  min-width: 210px;
}

.nd-node-card:hover {
  transform: translateY(-1px);
  box-shadow: var(--shadow-3);
}

.nd-path-scroll {
  overflow-x: auto;
  overflow-y: hidden;
  -webkit-overflow-scrolling: touch;
  padding-bottom: 0.25rem;
}

::v-deep(.nd-path-timeline .p-timeline-event) {
  flex: 0 0 auto;
}

::v-deep(.nd-path-timeline .p-timeline-event-content),
::v-deep(.nd-path-timeline .p-timeline-event-opposite) {
  flex: 0 0 auto;
}

::v-deep(.nd-path-timeline .p-timeline) {
  min-width: max-content;
}

.nd-path-scroll {
  overflow-x: auto;
  overflow-y: hidden;
  -webkit-overflow-scrolling: touch;

  /* Firefox */
  scrollbar-width: thin;
  scrollbar-color: rgba(255, 255, 255, 0.18) transparent;
}

/* WebKit (Chromium / Safari / WebView) */
.nd-path-scroll::-webkit-scrollbar {
  height: 8px;
}

.nd-path-scroll::-webkit-scrollbar-track {
  background: transparent;
}

.nd-path-scroll::-webkit-scrollbar-thumb {
  background-color: transparent;
  border-radius: 9999px;
  border: 2px solid transparent;
  background-clip: padding-box;
}

.nd-path-scroll:hover::-webkit-scrollbar-thumb {
  background-color: rgba(255, 255, 255, 0.28);
}

.nd-path-scroll:active::-webkit-scrollbar-thumb {
  background-color: rgba(255, 255, 255, 0.36);
}

.nd-marker {
  width: 24px;
  height: 24px;
}

::v-deep(.nd-path-timeline .p-timeline-event-separator) {
  min-height: 24px;
}

::v-deep(.nd-path-timeline.p-timeline) {
  padding-top: 0.5rem !important;
  padding-bottom: 0 !important;
}

::v-deep(.nd-path-timeline .p-timeline-event) {
  padding: 0 !important;
  margin: 0 !important;
}

::v-deep(.nd-path-timeline .p-timeline-event-opposite),
::v-deep(.nd-path-timeline .p-timeline-event-content) {
  padding: 0 !important;
  margin: 0 !important;
  min-height: 0 !important;
}

::v-deep(.nd-path-timeline .p-timeline-event-content) {
  padding-top: 0.25rem !important;
}

.nd-card-title {
  font-size: 0.95rem;
  font-weight: 600;
  line-height: 1.2;
  letter-spacing: 0.2px;
}

</style>

<template>
  <div
    ref="wrapRef"
    class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0"
  >
    <!-- Toolbar -->
    <div
      ref="toolbarRef"
      class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2"
    >
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm"
          >Overview</span
        >
        <div v-if="!sys" class="text-surface-500">Loading...</div>
        <div v-else class="text-surface-500 text-sm">
          <div v-if="hostnameVisible">
            <span
              class="text-surface-500 dark:text-surface-400 text-sm mr-3"
              >{{ sys.hostname }}</span
            >
          </div>
        </div>
      </div>
      <div class="flex items-center gap-2 justify-end">
        <Button
          outlined
          :icon="publicIpVisible ? 'pi pi-eye' : 'pi pi-eye-slash'"
          @click="togglePrivacy"
          class="w-9 h-9"
          severity="secondary"
          title="Toggle privacy filters"
        />
        <Button
          outlined
          :icon="autoInternetCheck ? 'pi pi-globe' : 'pi pi-times-circle'"
          @click="toggleAutoInternetCheck"
          class="w-9 h-9"
          severity="secondary"
          :title="autoInternetCheck ? 'Auto Internet Check: ON' : 'Auto Internet Check: OFF'"
        />
        <Button
          outlined
          icon="pi pi-refresh"
          :loading="loading"
          @click="fetchAll(); refreshIpInfo(true)"
          class="w-9 h-9"
          severity="secondary"
          title="Refresh data manually"
        />
      </div>
    </div>

    <div class="flex-1 min-h-0">
      <!-- Scrollable content -->
      <ScrollPanel
        :style="{ width: '100%', height: panelHeight }"
        class="flex-1 min-h-0"
      >
        <div
          class="grid grid-cols-1 xl:grid-cols-2 gap-2 content-start auto-rows-max p-1 items-stretch"
        >
          <!-- Network Path -->
          <Card class="xl:col-span-2 nd-path-card">
            <template #title>
              <div class="nd-card-title flex items-center gap-2">
                <i class="pi pi-share-alt text-surface-500"></i>
                <span>Network Path</span>
                <span class="text-xs text-surface-500 ml-2">Click a node to open details</span>
              </div>
            </template>
            <template #content>
              <div class="nd-path-scroll">
                <Timeline
                  :value="pathNodes"
                  layout="horizontal"
                  align="top"
                  class="nd-path-timeline"
                >
                  <template #marker="{ item }">
                    <span class="nd-marker" :style="{ backgroundColor: item.color }">
                      <i :class="item.icon"></i>
                    </span>
                  </template>

                  <template #content="{ item }">
                    <Card class="nd-node-card" @click="onPathNodeClick(item)">
                      <template #title>
                        <div class="text-sm font-semibold">{{ item.title }}</div>
                      </template>
                      <template #subtitle>
                        <div
                          class="text-xs font-mono truncate"
                          :class="{ 'text-surface-500': !publicIpVisible }"
                        >
                          {{ item.subtitle ?? "" }}
                        </div>
                      </template>
                      <template #content>
                        <div class="text-xs text-surface-500 line-clamp-2">
                          {{ item.summary ?? "" }}
                        </div>
                      </template>
                    </Card>
                  </template>
                </Timeline>
              </div>
            </template>
          </Card>
          <!-- Default Interface -->
          <Card class="h-full">
            <template #title>
              <div class="nd-card-title flex items-center justify-between gap-2">
                <!-- left -->
                <div class="flex items-center gap-2 min-w-0">
                  <i class="pi pi-arrows-h text-surface-500"></i>
                  <span>Default Interface</span>
                </div>

                <!-- right -->
                <div v-if="defaultIface" class="flex items-center gap-2 min-w-0">
                  <span class="text-sm truncate">
                    {{ defaultIface.display_name }}
                  </span>
                  <Tag
                    class="text-[11px]! py-0.5!"
                    v-if="defaultIface.oper_state"
                    :value="defaultIface.oper_state"
                    :severity="severityByOper(defaultIface.oper_state)"
                  />
                </div>

                <div v-else class="text-xs text-surface-500">
                  No default interface
                </div>
              </div>
            </template>
            <template #content>
              <div class="flex flex-col gap-4 text-sm h-full">
                <div v-if="!defaultIface" class="text-surface-500">
                  No default interface detected.
                </div>
                <div v-else class="flex flex-col gap-4 text-sm">
                  <!-- Overview / Performance -->
                  <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <!-- Overview -->
                    <div
                      class="rounded-xl border border-surface-200 dark:border-surface-700 p-3"
                    >
                      <div class="font-semibold mb-2">Overview</div>
                      <div class="grid grid-cols-[90px_1fr] gap-x-3 gap-y-1 text-sm">
                        <div class="text-surface-500">Index:</div><div class="font-mono">{{ defaultIface.index }}</div>
                        <div class="text-surface-500">Type:</div><div>{{ fmtIfType(defaultIface.if_type) }}</div>
                        <div class="text-surface-500">Friendly:</div><div class="truncate">{{ defaultIface.friendly_name ?? "-" }}</div>
                        <div class="text-surface-500">Description:</div><div class="truncate">{{ defaultIface.description ?? "-" }}</div>
                        <div class="text-surface-500">MAC:</div><div class="font-mono">{{ maskMac(defaultIface.mac_addr) }}</div>
                        <div class="text-surface-500">MTU:</div><div>{{ defaultIface.mtu ?? "-" }}</div>
                      </div>
                    </div>

                    <!-- Performance -->
                    <div
                      class="rounded-xl border border-surface-200 dark:border-surface-700 p-3"
                    >
                      <div class="font-semibold mb-2">Performance</div>
                      <div class="grid grid-cols-2 gap-3">
                        <div
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-2"
                        >
                          <div class="text-surface-500 text-xs">
                            {{ rxLabel }}
                          </div>
                          <div class="text-base font-semibold">
                            {{
                              formatThroughput(
                                defaultIface.stats?.rx_bytes_per_sec || 0,
                              )
                            }}
                          </div>
                        </div>
                        <div
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-2"
                        >
                          <div class="text-surface-500 text-xs">
                            {{ txLabel }}
                          </div>
                          <div class="text-base font-semibold">
                            {{
                              formatThroughput(
                                defaultIface.stats?.tx_bytes_per_sec || 0,
                              )
                            }}
                          </div>
                        </div>
                        <div
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-2"
                        >
                          <div class="text-surface-500 text-xs">
                            RX total bytes
                          </div>
                          <div class="font-mono">
                            {{ formatBytes(defaultIface.stats?.rx_bytes || 0) }}
                          </div>
                        </div>
                        <div
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-2"
                        >
                          <div class="text-surface-500 text-xs">
                            TX total bytes
                          </div>
                          <div class="font-mono">
                            {{ formatBytes(defaultIface.stats?.tx_bytes || 0) }}
                          </div>
                        </div>
                      </div>
                      <div class="text-xs text-surface-500 mt-1">
                        Link Speed:
                        <span v-if="defaultIface.receive_speed">
                          RX {{ formatBps(defaultIface.receive_speed) }}
                        </span>
                        <span v-else>RX -</span>
                        /
                        <span v-if="defaultIface.transmit_speed">
                          TX {{ formatBps(defaultIface.transmit_speed) }}
                        </span>
                        <span v-else>TX -</span>
                      </div>
                    </div>
                  </div>

                  <!-- IP Addresses -->
                  <div
                    class="rounded-xl border border-surface-200 dark:border-surface-700 p-3"
                  >
                    <div class="font-semibold mb-2">IP Addresses</div>
                    <div class="mb-2">
                      <span class="text-surface-500 text-xs">IPv4</span>
                      <div class="mt-1 flex flex-wrap gap-2">
                        <Chip
                          v-for="(v, i) in defaultIface.ipv4 ?? []"
                          :key="'v4-' + i"
                          :label="maskIpLabel(v)"
                          :class="['font-mono', 'copyable', !publicIpVisible && 'text-surface-500']"
                        />
                        <span
                          v-if="(defaultIface.ipv4?.length ?? 0) === 0"
                          >-</span
                        >
                      </div>
                    </div>
                    <div>
                      <span class="text-surface-500 text-xs">IPv6</span>
                      <div class="mt-1 flex flex-wrap gap-2">
                        <Chip
                          v-for="(v, i) in defaultIface.ipv6 ?? []"
                          :key="'v6-' + i"
                          :label="maskIpLabel(v)"
                          :class="['font-mono', 'copyable', !publicIpVisible && 'text-surface-500']"
                        />
                        <span
                          v-if="(defaultIface.ipv6?.length ?? 0) === 0"
                          >-</span
                        >
                      </div>
                    </div>
                  </div>
                </div>
                <!-- /else -->
                 <div class="flex-1"></div>
              </div>
            </template>
          </Card>

          <!-- Live Traffic -->
          <Card class="h-full">
            <template #title>
              <div class="nd-card-title flex items-center justify-between gap-2">
                <!-- left -->
                <div class="flex items-center gap-2 min-w-0">
                  <i class="pi pi-chart-line text-surface-500"></i>
                  <span>Live Traffic</span>
                </div>

                <!-- right -->
                <div class="flex items-center gap-2 min-w-0">
                  <span v-if="defaultIface" class="text-sm truncate">
                    {{ defaultIface.display_name }}
                  </span>
                  <span v-else class="text-xs text-surface-500">
                    No default interface
                  </span>

                  <span class="text-xs text-surface-500 hidden sm:inline">
                    Real-time RX/TX
                  </span>
                </div>
              </div>
            </template>
            <template #content>
              <div class="flex flex-col gap-3 text-sm h-full">
                <div class="flex-1 min-h-75">
                  <Chart
                    type="line"
                    :data="trafficData"
                    :options="trafficOptions"
                    class="w-full h-full"
                  />
                </div>
                <div class="grid grid-cols-2 gap-3 mt-2">
                  <!-- RX stats -->
                  <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs mb-1">
                      {{ rxLabel }} stats
                    </div>
                    <div class="grid grid-cols-3 gap-2 text-xs">
                      <div>
                        <div class="text-surface-500 text-[11px]">AVG</div>
                        <div class="font-semibold text-sm">
                          {{ formatStat(rxStats?.avg) }}
                        </div>
                      </div>
                      <div>
                        <div class="text-surface-500 text-[11px]">MAX</div>
                        <div class="font-semibold text-sm">
                          {{ formatStat(rxStats?.max) }}
                        </div>
                      </div>
                    </div>
                  </div>

                  <!-- TX stats -->
                  <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs mb-1">
                      {{ txLabel }} stats
                    </div>
                    <div class="grid grid-cols-3 gap-2 text-xs">
                      <div>
                        <div class="text-surface-500 text-[11px]">AVG</div>
                        <div class="font-semibold text-sm">
                          {{ formatStat(txStats?.avg) }}
                        </div>
                      </div>
                      <div>
                        <div class="text-surface-500 text-[11px]">MAX</div>
                        <div class="font-semibold text-sm">
                          {{ formatStat(txStats?.max) }}
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </template>
          </Card>
        </div>
      </ScrollPanel>
    </div>
  </div>

  <Dialog
    v-model:visible="pathDetailOpen"
    modal
    :dismissableMask="true"
    :draggable="false"
    :style="{ width: 'min(720px, 92vw)' }"
  >
    <template #header>
      <div class="flex items-center gap-2">
        <i
          class="pi"
          :class="pathDetailKind === 'gateway' ? 'pi-directions' : 'pi-server'"
        />
        <span class="font-semibold">
          {{ pathDetailKind === 'gateway' ? 'Gateway Details' : 'DNS Details' }}
        </span>
      </div>
    </template>

    <!-- Gateway -->
    <div v-if="pathDetailKind === 'gateway'" class="text-sm space-y-3">
      <div v-if="!gwDetail" class="text-surface-500">No gateway detected.</div>

      <div v-else class="space-y-3">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div class="rounded-lg border border-surface-200 dark:border-surface-700 p-3">
            <div class="text-xs text-surface-500 mb-1">MAC</div>
            <div class="font-mono">
              {{ maskMac(gwDetail.mac) }}
            </div>
          </div>

          <div class="rounded-lg border border-surface-200 dark:border-surface-700 p-3">
            <div class="text-xs text-surface-500 mb-1">From Interface</div>
            <div class="font-mono">
              {{ defaultIface?.name ?? '-' }}
            </div>
          </div>
        </div>

        <div class="rounded-lg border border-surface-200 dark:border-surface-700 p-3">
          <div class="text-xs text-surface-500 mb-2">IPv4</div>
          <div class="flex flex-wrap gap-2">
            <Chip
              v-for="(ip, i) in gwDetail.ipv4"
              :key="'gw4-' + i"
              :label="pubIpGate(ip)"
              class="font-mono copyable"
              :class="{ 'text-surface-500': !publicIpVisible }"
            />
            <span v-if="gwDetail.ipv4.length === 0">-</span>
          </div>

          <div class="text-xs text-surface-500 mt-3 mb-2">IPv6</div>
          <div class="flex flex-wrap gap-2">
            <Chip
              v-for="(ip, i) in gwDetail.ipv6"
              :key="'gw6-' + i"
              :label="pubIpGate(ip)"
              class="font-mono copyable"
              :class="{ 'text-surface-500': !publicIpVisible }"
            />
            <span v-if="gwDetail.ipv6.length === 0">-</span>
          </div>
        </div>

        <div class="flex justify-end gap-2 pt-1">
          <Button
            label="Check Routes"
            icon="pi pi-external-link"
            severity="secondary"
            outlined
            @click="router.push({ name: 'routes' }); pathDetailOpen=false;"
          />
        </div>
      </div>
    </div>

    <!-- DNS -->
    <div v-else-if="pathDetailKind === 'dns'" class="text-sm space-y-3">
      <div class="rounded-lg border border-surface-200 dark:border-surface-700 p-3">
        <div class="text-xs text-surface-500 mb-2">DNS Servers</div>
        <div class="flex flex-wrap gap-2">
          <Chip
            v-for="(d, i) in dnsDetail.servers"
            :key="'dns-' + i"
            :label="pubIpGate(d)"
            class="font-mono copyable"
            :class="{ 'text-surface-500': !publicIpVisible }"
          />
          <span v-if="dnsDetail.servers.length === 0">-</span>
        </div>
      </div>

      <div class="flex justify-end gap-2 pt-1">
        <Button
          label="Check DNS"
          icon="pi pi-external-link"
          severity="secondary"
          outlined
          @click="router.push({ name: 'dns' }); pathDetailOpen=false;"
        />
      </div>
    </div>
  </Dialog>
</template>
