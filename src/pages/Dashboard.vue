<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { NetworkInterface } from "../types/net";
import { formatBps, formatBytesPerSec, formatBytes } from "../types/net";
import type { SysInfo } from "../types/system";
import { fmtIfType, hexFlags, severityByOper } from "../utils/formatter";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { usePrivacyGate } from "../composables/usePrivacyGate";
import type { ChartData, ChartOptions } from "chart.js";
import { hexToRgba } from "../utils/color";
import { readBpsUnit, type UnitPref } from "../utils/preferences";

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
      title: { display: true, text: "Time" },
      ticks: {
        color: textColorSecondary
      },
      grid: {
        color: surfaceBorder
      }
    },
    y: {
      beginAtZero: true,
      grace: '5%',
      suggestedMax: 1_000, // Initial max
      ticks: {
        callback(value) {
          const num = typeof value === "number" ? value : Number(value);
          return formatAxisThroughput(Number.isFinite(num) ? num : 0);
        },
        color: textColorSecondary
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

onMounted(async () => {
  initTrafficChart();
  refreshUnitPref();
  await fetchAll();
  unlistenStats = await listen("stats_updated", onStatsUpdated);
  unlistenIfaces = await listen("interfaces_updated", onInterfacesUpdated);
  window.addEventListener("storage", refreshUnitPref);
});
onBeforeUnmount(() => {
  unlistenStats?.();
  unlistenIfaces?.();
  window.removeEventListener("storage", refreshUnitPref);
});
</script>

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
        />
        <Button
          outlined
          icon="pi pi-refresh"
          :loading="loading"
          @click="fetchAll"
          class="w-9 h-9"
          severity="secondary"
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
          class="grid grid-cols-1 xl:grid-cols-2 gap-2 content-start auto-rows-max p-1"
        >
          <!-- Default Interface -->
          <Card>
            <template #title>Default Interface</template>
            <template #content>
              <div class="flex flex-col gap-4 text-sm">
                <div v-if="!defaultIface" class="text-surface-500">
                  No default interface detected.
                </div>

                <div v-else class="flex flex-col gap-4 text-sm">
                  <!-- Header -->
                  <div class="flex items-center gap-3">
                    <i class="pi pi-arrows-h text-surface-500"></i>
                    <span class="font-bold truncate">
                      {{ defaultIface.name }}
                    </span>
                    <Tag
                      v-if="defaultIface.oper_state"
                      :value="defaultIface.oper_state"
                      :severity="severityByOper(defaultIface.oper_state)"
                    />
                    <Tag
                      v-if="defaultIface.default"
                      value="Default"
                      severity="info"
                    />
                  </div>

                  <!-- Overview / Performance -->
                  <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <!-- Overview -->
                    <div
                      class="rounded-xl border border-surface-200 dark:border-surface-700 p-3"
                    >
                      <div class="font-semibold mb-2">Overview</div>
                      <div class="space-y-1">
                        <div>
                          <span class="text-surface-500">Index:</span>
                          <span class="font-mono">
                            {{ defaultIface.index }}
                          </span>
                        </div>
                        <div>
                          <span class="text-surface-500">Type:</span>
                          <span>{{ fmtIfType(defaultIface.if_type) }}</span>
                        </div>
                        <div>
                          <span class="text-surface-500">Friendly:</span>
                          <span>
                            {{ defaultIface.friendly_name ?? "-" }}
                          </span>
                        </div>
                        <div>
                          <span class="text-surface-500">Description:</span>
                          <span>
                            {{ defaultIface.description ?? "-" }}
                          </span>
                        </div>
                        <div>
                          <span class="text-surface-500">MAC:</span>
                          <span
                            class="font-mono copyable"
                            :class="{ 'text-surface-500': !publicIpVisible }"
                          >
                            {{ maskMac(defaultIface.mac_addr) }}
                          </span>
                        </div>
                        <div>
                          <span class="text-surface-500">MTU:</span>
                          <span>{{ defaultIface.mtu ?? "-" }}</span>
                        </div>
                        <div>
                          <span class="text-surface-500">Flags:</span>
                          <span class="font-mono">
                            {{ hexFlags(defaultIface.flags) }}
                          </span>
                        </div>
                      </div>
                    </div>

                    <!-- Performance -->
                    <div
                      class="rounded-xl border border-surface-200 dark:border-surface-700 p-3"
                    >
                      <div class="font-semibold mb-2">Performance</div>
                      <div class="grid grid-cols-2 gap-3">
                        <div
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3"
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
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3"
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
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3"
                        >
                          <div class="text-surface-500 text-xs">
                            RX total bytes
                          </div>
                          <div class="font-mono">
                            {{ formatBytes(defaultIface.stats?.rx_bytes || 0) }}
                          </div>
                        </div>
                        <div
                          class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3"
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
                      <div
                        class="text-xs text-surface-500 mt-2"
                        v-if="(defaultIface.ipv6_scope_ids?.length ?? 0) > 0"
                      >
                        Scope IDs:
                        <span class="font-mono">
                          {{
                            (defaultIface.ipv6_scope_ids ?? []).join(", ")
                          }}
                        </span>
                      </div>
                    </div>
                  </div>

                  <!-- Routing / DNS -->
                  <div
                    class="rounded-xl border border-surface-200 dark:border-surface-700 p-3"
                  >
                    <div class="font-semibold mb-2">Routing / DNS</div>
                    <div class="flex flex-wrap gap-6">
                      <div>
                        <div class="text-surface-500 text-xs">Gateway</div>
                        <div class="mt-1">
                          <template v-if="defaultIface.gateway">
                            MAC: 
                            <span
                              class="font-mono copyable"
                              :class="{ 'text-surface-500': !publicIpVisible }"
                            >
                              {{ maskMac(defaultIface.gateway.mac_addr) }}
                            </span>
                            <div v-if="defaultIface.gateway.ipv4.length > 0">
                              IPv4:
                              <span
                                class="font-mono copyable"
                                :class="{ 'text-surface-500': !publicIpVisible }"
                              >
                                {{ pubIpGate(defaultIface.gateway.ipv4.join(", ")) }}
                              </span>
                            </div>
                            <div v-if="defaultIface.gateway.ipv6.length > 0">
                              IPv6:
                              <span
                                class="font-mono copyable"
                                :class="{ 'text-surface-500': !publicIpVisible }"
                              >
                                {{ pubIpGate(defaultIface.gateway.ipv6.join(", ")) }}
                              </span>
                            </div>
                          </template>
                          <span v-else>-</span>
                        </div>
                      </div>
                      <div>
                        <div class="text-surface-500 text-xs">DNS</div>
                        <div class="mt-1 flex flex-wrap gap-2">
                          <Chip
                            v-for="(d, i) in defaultIface.dns_servers ?? []"
                            :key="'dns-' + i"
                            :label="d"
                            class="font-mono copyable"
                          />
                          <span
                            v-if="(defaultIface.dns_servers?.length ?? 0) === 0"
                            >-</span
                          >
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
                <!-- /else -->
              </div>
            </template>
          </Card>

          <!-- Live Traffic -->
          <Card>
            <template #title>
              <div v-if="defaultIface">
                Live Traffic - {{ defaultIface.display_name }}
              </div>
              <div v-else>
                Live Traffic (No default interface)
              </div>
            </template>
            <template #content>
              <div class="flex flex-col gap-3 text-sm">
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-2">
                    <i class="pi pi-chart-line text-surface-500"></i>
                    <span class="text-surface-500">
                      Real-time RX/TX throughput
                    </span>
                  </div>
                  <div class="text-xs text-surface-500">
                    Unit:
                    <span class="font-mono">{{ bpsUnit }}</span>
                  </div>
                </div>

                <Chart
                  type="line"
                  :data="trafficData"
                  :options="trafficOptions"
                  :height="320"
                  class="w-full"
                />

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
</template>
