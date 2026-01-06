<script setup lang="ts">
import {
  ref,
  reactive,
  computed,
  onMounted,
  onBeforeUnmount,
  nextTick,
} from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

import type {
  TraceProtocol,
  TraceHop,
  TraceSetting,
  TraceDonePayload,
} from "../types/probe";
import type { Host } from "../types/net";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import type { ChartData, ChartOptions } from "chart.js";

const form = reactive({
  protocol: "Icmp" as TraceProtocol,
  host: "1.1.1.1",
  max_hops: 30,
  tries_per_hop: 2,
  timeout_ms: 2000,
});

const running = ref(false);
const loading = ref(false);
const err = ref<string | null>(null);

// Progress hops
const hops = ref<TraceHop[]>([]);

// Summary at done
const doneInfo = ref<TraceDonePayload | null>(null);
// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();

// Chart data
const chartData = ref<ChartData<"line">>({
  labels: [],
  datasets: [
    {
      label: "RTT (ms)",
      data: [],
      fill: false,
      tension: 0.25,
    },
  ],
});

const chartOptions = ref<ChartOptions<"line">>({
  responsive: true,
  maintainAspectRatio: false,
  animation: false,
  scales: {
    x: {
      title: { display: true, text: "Hop" },
    },
    y: {
      beginAtZero: true,
      // Hint for latency
      suggestedMax: 50,
      ticks: {
        callback(value) {
          return `${value} ms`;
        },
      },
    },
  },
});

function pxToNumber(px: string | number | null | undefined): number {
  if (px == null) return 0;
  if (typeof px === "number") return px;
  const m = String(px).match(/(\d+(\.\d+)?)/);
  return m ? Number(m[1]) : 0;
}

const hopsTableHeight = computed(() => {
  const ph = pxToNumber(panelHeight.value);
  const RESERVED = 160;
  const h = Math.max(200, ph - RESERVED);
  return `${Math.floor(h)}px`;
});

function resetResult() {
  hops.value = [];
  doneInfo.value = null;
  err.value = null;

  chartData.value.labels = [];
  chartData.value.datasets[0].data = [];
}

async function resolveTarget(target: string): Promise<Host> {
  const host: Host = await invoke("lookup_host", { host: target });
  return host;
}

async function toTraceSetting(): Promise<TraceSetting> {
  const host = await resolveTarget(form.host);
  return {
    hostname: host.hostname ?? null,
    ip_addr: host.ip,
    protocol: form.protocol.toLowerCase() as TraceProtocol,
    max_hops: form.max_hops,
    tries_per_hop: form.tries_per_hop,
    timeout_ms: form.timeout_ms,
  };
}

async function startTrace() {
  resetResult();
  running.value = true;
  loading.value = true;

  try {
    const setting = await toTraceSetting();
    await invoke("traceroute", { setting });
  } catch (e: any) {
    err.value = String(e?.message ?? e);
    running.value = false;
  } finally {
    loading.value = false;
  }
}

let unlistenStart: UnlistenFn | null = null;
let unlistenProgress: UnlistenFn | null = null;
let unlistenDone: UnlistenFn | null = null;
let unlistenError: UnlistenFn | null = null;

onMounted(async () => {
  await nextTick();

  // start
  unlistenStart = await listen("traceroute:start", () => {
    resetResult();
    running.value = true;
  });

  // progress: each hop
  unlistenProgress = await listen("traceroute:progress", (ev: any) => {
    const hop: TraceHop | undefined = ev?.payload;
    if (!hop) return;

    hops.value = [...hops.value, hop];

    const current = chartData.value;

    const labels = [...(current.labels ?? []), String(hop.hop)];
    const data = [
        ...((current.datasets?.[0].data as (number | null)[] | undefined) ?? []),
        hop.rtt_ms != null ? hop.rtt_ms : null,
    ];

    chartData.value = {
        ...current,
        labels,
        datasets: [
        {
            ...current.datasets?.[0],
            data,
        } as any,
        ],
    };
    });

  // done
  unlistenDone = await listen("traceroute:done", (ev: any) => {
    const payload: TraceDonePayload | undefined = ev?.payload;
    if (payload) {
      doneInfo.value = payload;
    }
    running.value = false;
  });

  // error
  unlistenError = await listen("traceroute:error", (ev: any) => {
    const p = ev?.payload ?? {};
    if (p.message) {
      err.value = String(p.message);
      running.value = false;
    }
  });
});

onBeforeUnmount(() => {
  unlistenStart?.();
  unlistenProgress?.();
  unlistenDone?.();
  unlistenError?.();
});

// Whether reached the target. The final result.
const reached = computed(() => !!doneInfo.value?.reached);

// Last hop info. If reached, the hop that reached the target. Otherwise, the last hop.
const lastHop = computed(() => {
  if (!hops.value.length) return null;
  if (reached.value) {
    const r = hops.value.find((h) => h.reached);
    if (r) return r;
  }
  return hops.value[hops.value.length - 1];
});

function fmtMs(v?: number | null) {
  if (v == null) return "-";
  return `${v} ms`;
}

function fmtIp(ip?: string | null) {
  if (!ip) return "*";
  return ip;
}
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div
      ref="toolbarRef"
      class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-3"
    >
      <!-- Left: controls -->
      <div class="flex flex-wrap items-end gap-3 min-w-0">
        <!-- Protocol -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Protocol</label>
          <Select
            v-model="form.protocol"
            :options="[
              { label: 'ICMP', value: 'Icmp' },
              { label: 'UDP',  value: 'Udp'  },
            ]"
            optionLabel="label"
            optionValue="value"
            class="min-w-[120px]"
            aria-label="Protocol"
          />
        </div>

        <!-- Host -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Host / IP</label>
          <InputText
            v-model="form.host"
            placeholder="host or IP"
            class="w-[220px]"
            aria-label="Host or IP address"
          />
        </div>

        <!-- Max hops -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Max hops</label>
          <InputNumber
            v-model="form.max_hops"
            :min="1"
            :max="64"
            inputClass="w-[110px]"
            aria-label="Maximum hops"
          />
        </div>

        <!-- Tries / hop -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Probes / hop</label>
          <InputNumber
            v-model="form.tries_per_hop"
            :min="1"
            :max="5"
            inputClass="w-[120px]"
            aria-label="Number of probes per hop"
          />
        </div>

        <!-- Timeout -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Timeout (ms)</label>
          <InputNumber
            v-model="form.timeout_ms"
            :min="100"
            :max="10000"
            :step="100"
            inputClass="w-[130px]"
            aria-label="Timeout in milliseconds"
          />
        </div>
      </div>

      <!-- Right: actions -->
      <div class="flex flex-wrap items-center gap-3 justify-end">
        <Button
          label="Start"
          icon="pi pi-play"
          :disabled="running || !form.host?.trim()"
          :loading="loading"
          @click="startTrace"
          aria-label="Start traceroute"
        />
      </div>
    </div>

    <!-- Main content -->
    <div class="flex-1 min-h-0">
      <ScrollPanel
        :style="{ width: '100%', height: panelHeight }"
        class="flex-1 min-h-0"
      >
        <div class="grid grid-cols-1 xl:grid-cols-2 gap-3">
          <!-- Left: Chart + Basic info -->
          <Card>
            <template #title>Trace Chart</template>
            <template #content>
              <div class="mb-3 text-sm text-surface-500">
                <span v-if="doneInfo">
                  Target:
                  <span class="font-mono">
                    {{
                      doneInfo.hostname
                        ? `${doneInfo.hostname} (${doneInfo.ip_addr})`
                        : doneInfo.ip_addr
                    }}
                  </span>
                  - Protocol: {{ doneInfo.protocol }}
                  - Reached: {{ reached ? "Yes" : "No" }}
                </span>
                <span v-else>
                  Configure target and start traceroute to see route and
                  latency.
                </span>
              </div>

              <Chart type="line" :data="chartData" :options="chartOptions" />

              <div class="mt-3 text-xs text-surface-500">
                <div v-if="lastHop">
                  Last hop: #{{ lastHop.hop }}
                  ({{ fmtIp(lastHop.ip_addr as any) }})
                  - RTT: {{ fmtMs(lastHop.rtt_ms as any) }}
                </div>
              </div>

              <div v-if="err" class="mt-3 text-red-500 text-sm">
                {{ err }}
              </div>
            </template>
          </Card>

          <!-- Right: Hop table -->
          <Card>
            <template #title>Hops</template>
            <template #content>
              <DataTable
                :value="hops"
                size="small"
                scrollable
                :scrollHeight="hopsTableHeight"
                class="text-sm copyable"
              >
                <Column field="hop" header="#" style="width: 60px" />

                <Column header="IP">
                  <template #body="{ data }">
                    <span class="font-mono">
                      {{ fmtIp(data.ip_addr) }}
                    </span>
                  </template>
                </Column>

                <Column header="RTT">
                  <template #body="{ data }">
                    {{ fmtMs(data.rtt_ms) }}
                  </template>
                </Column>

                <Column header="Status" style="width: 100px">
                  <template #body="{ data }">
                    <Tag
                      v-if="data.reached"
                      value="REACHED"
                      severity="success"
                    />
                    <Tag
                      v-else-if="data.note === 'timeout'"
                      value="TIMEOUT"
                      severity="warn"
                    />
                    <Tag
                      v-else
                      value="HOP"
                      severity="info"
                    />
                  </template>
                </Column>

                <Column header="Note">
                  <template #body="{ data }">
                    <span class="text-surface-500">
                      {{ data.note ?? "-" }}
                    </span>
                  </template>
                </Column>
              </DataTable>
            </template>
          </Card>
        </div>
      </ScrollPanel>
    </div>
  </div>
</template>
