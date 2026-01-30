<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import Chart from 'primevue/chart';
import type { ChartData, ChartOptions } from "chart.js";
import type { NetworkInterface } from "../types/net";
import { severityByOper, fmtBps, fmtBytesPerSec } from "../utils/formatter";
import { hexToRgba } from "../utils/color";
import { readBpsUnit, type UnitPref } from "../utils/preferences";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";

type TrafficSample = { ts: number; rx: number; tx: number };

const ifaces = ref<NetworkInterface[]>([]);
const selectedIndexes = ref<number[]>([]);

const histories = reactive<Record<number, TrafficSample[]>>({});

const bpsUnit = ref<UnitPref>(readBpsUnit(localStorage));

function refreshUnitPref() {
  bpsUnit.value = readBpsUnit(localStorage);
}

function fmtThroughput(v?: number): string {
  const n = v ?? 0;
  return bpsUnit.value === "bits" ? fmtBps(n * 8) : fmtBytesPerSec(n);
}

async function fetchInterfaces() {
  const data = (await invoke("get_network_interfaces")) as NetworkInterface[];
  ifaces.value = data;
}

function pushHistorySample() {
  const now = Date.now();

  for (const iface of ifaces.value) {
    const idx = iface.index;
    const rx = iface.stats?.rx_bytes_per_sec ?? 0;
    const tx = iface.stats?.tx_bytes_per_sec ?? 0;

    if (!histories[idx]) histories[idx] = [];

    const arr = histories[idx];
    arr.push({ ts: now, rx, tx });

    if (arr.length > 30) {
      arr.splice(0, arr.length - 30);
    }
  }
}

let unlistenStats: UnlistenFn | null = null;
let debouncing = false;

async function onStatsUpdated() {
  if (debouncing) return;
  debouncing = true;
  setTimeout(async () => {
    refreshUnitPref();
    await fetchInterfaces();
    pushHistorySample();
    debouncing = false;
  }, 500);
}

onMounted(async () => {
  refreshUnitPref();
  await fetchInterfaces();
  pushHistorySample();
  unlistenStats = await listen("stats_updated", onStatsUpdated);
  window.addEventListener("storage", refreshUnitPref);
});

onBeforeUnmount(() => {
  unlistenStats?.();
  window.removeEventListener("storage", refreshUnitPref);
});

const interfaceOptions = computed(() => {
  const list = [...ifaces.value];

  const defaultIdx = list.find((i) => i.default)?.index;

  return list
    .sort((a, b) => {
      // 1. default
      if (a.index === defaultIdx && b.index !== defaultIdx) return -1;
      if (b.index === defaultIdx && a.index !== defaultIdx) return 1;

      // 2. oper_state === "up"
      const aUp = (a.oper_state ?? "").toLowerCase() === "up" ? 1 : 0;
      const bUp = (b.oper_state ?? "").toLowerCase() === "up" ? 1 : 0;
      if (aUp !== bUp) return bUp - aUp;

      // 3. display_name / name
      const aName = (a.display_name ?? a.name ?? "").toLowerCase();
      const bName = (b.display_name ?? b.name ?? "").toLowerCase();
      return aName.localeCompare(bName);
    })
    .map((i) => ({
      label: i.display_name ?? i.name,
      value: i.index,
    }));
});

// Selected interfaces
const monitoredIfaces = computed(() =>
  selectedIndexes.value
    .map(idx => ifaces.value.find(i => i.index === idx))
    .filter((i): i is NetworkInterface => !!i)
);

// Chart.js options
const documentStyle = getComputedStyle(document.documentElement);
const textColor = documentStyle.getPropertyValue("--p-text-color");
const surfaceBorder = documentStyle.getPropertyValue("--p-content-border-color");
const rxBorder = documentStyle.getPropertyValue("--p-cyan-400").trim();
const txBorder = documentStyle.getPropertyValue("--p-pink-400").trim();

const miniChartOptions: ChartOptions<"line"> = {
  responsive: true,
  maintainAspectRatio: false,
  animation: false,
  plugins: {
    legend: { display: false },
    tooltip: {
      callbacks: {
        label(ctx) {
          const raw = ctx.parsed.y ?? 0;
          return fmtThroughput(raw);
        },
      },
    },
  },
  scales: {
    x: {
      display: false,
      grid: { display: false },
    },
    y: {
      beginAtZero: true,
      ticks: {
        color: textColor,
        callback(v) {
          const num = typeof v === "number" ? v : Number(v);
          return fmtThroughput(num);
        },
      },
      grid: {
        color: surfaceBorder,
      },
    },
  },
};

function miniChartData(iface: NetworkInterface): ChartData<"line"> {
  const hist = histories[iface.index] ?? [];
  const labels = hist.map((s) => new Date(s.ts).toLocaleTimeString());
  const rxData = hist.map((s) => s.rx);
  const txData = hist.map((s) => s.tx);

  return {
    labels,
    datasets: [
      {
        label: "RX",
        data: rxData,
        borderColor: rxBorder,
        backgroundColor: hexToRgba(rxBorder, 0.15),
        fill: true,
        tension: 0.25,
      },
      {
        label: "TX",
        data: txData,
        borderColor: txBorder,
        backgroundColor: hexToRgba(txBorder, 0.15),
        fill: true,
        tension: 0.25,
      },
    ],
  };
}

function calcStats(hist: TrafficSample[]) {
  if (hist.length === 0) return null;
  const rxArr = hist.map((s) => s.rx);
  const txArr = hist.map((s) => s.tx);

  const mk = (arr: number[]) => {
    let min = arr[0],
      max = arr[0],
      sum = 0;
    for (const v of arr) {
      if (v < min) min = v;
      if (v > max) max = v;
      sum += v;
    }
    return { min, max, avg: sum / arr.length };
  };

  return { rx: mk(rxArr), tx: mk(txArr) };
}

function ifaceStats(iface: NetworkInterface) {
  return calcStats(histories[iface.index] ?? []);
}
// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();
</script>

<template>
  <div
    ref="wrapRef"
    class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0"
  >
    <!-- Toolbar -->
    <div
      ref="toolbarRef"
      class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-3"
    >
      <div class="flex items-center gap-2 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm">
          Traffic Monitor
        </span>
        <span class="text-xs text-surface-500">
          ({{ monitoredIfaces.length }} interfaces)
        </span>
      </div>

      <div class="flex flex-wrap items-center gap-2 justify-end">
        <MultiSelect
          v-model="selectedIndexes"
          :options="interfaceOptions"
          optionLabel="label"
          optionValue="value"
          placeholder="Select interfaces"
          class="min-w-48"
          display="chip"
          :maxSelectedLabels="3"
        />
        <span class="text-xs text-surface-500">
          Unit: <span class="font-mono">{{ bpsUnit }}</span>
        </span>
      </div>
    </div>

    <div class="flex-1 min-h-0">
      <!-- Scrollable content -->
      <ScrollPanel
        :style="{ width: '100%', height: panelHeight }"
        class="flex-1 min-h-0"
      >
        <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 p-1">
          <Card
            v-for="iface in monitoredIfaces"
            :key="iface.index"
          >
            <template #title>
              <div class="flex items-center gap-2 min-w-0">
                <span class="font-semibold truncate">
                  {{ iface.display_name ?? iface.name }}
                </span>
                <Tag
                  v-if="iface.oper_state"
                  :value="iface.oper_state"
                  :severity="severityByOper(iface.oper_state)"
                />
                <Tag
                  v-if="iface.default"
                  value="Default"
                  severity="info"
                />
              </div>
            </template>

            <template #content>
              <div class="flex flex-col gap-2 text-xs">
                <div class="h-24">
                  <Chart
                    type="line"
                    :data="miniChartData(iface)"
                    :options="miniChartOptions"
                    class="w-full h-full"
                  />
                </div>

                <div class="grid grid-cols-2 gap-2 mt-1">
                  <div>
                    <div class="text-surface-500 text-[11px]">RX now</div>
                    <div class="font-semibold text-sm">
                      {{ fmtThroughput(iface.stats?.rx_bytes_per_sec || 0) }}
                    </div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-[11px]">TX now</div>
                    <div class="font-semibold text-sm">
                      {{ fmtThroughput(iface.stats?.tx_bytes_per_sec || 0) }}
                    </div>
                  </div>
                </div>

                <div
                  v-if="ifaceStats(iface)"
                  class="grid grid-cols-2 gap-2 text-[11px] mt-1"
                >
                  <div>
                    <div class="text-surface-500">RX avg / max</div>
                    <div class="font-mono">
                      {{ fmtThroughput(ifaceStats(iface)!.rx.avg) }}
                      /
                      {{ fmtThroughput(ifaceStats(iface)!.rx.max) }}
                    </div>
                  </div>
                  <div>
                    <div class="text-surface-500">TX avg / max</div>
                    <div class="font-mono">
                      {{ fmtThroughput(ifaceStats(iface)!.tx.avg) }}
                      /
                      {{ fmtThroughput(ifaceStats(iface)!.tx.max) }}
                    </div>
                  </div>
                </div>
              </div>
            </template>
          </Card>
        </div>

        <div
          v-if="!monitoredIfaces.length"
          class="text-surface-500 text-sm mt-6 text-center"
        >
          Select one or more interfaces to start monitoring.
        </div>
      </ScrollPanel>
    </div>
  </div>
</template>
