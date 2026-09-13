<script setup lang="ts">
import { useDiagnosticRun } from "../composables/useDiagnosticRun";
import {
  ref,
  reactive,
  computed,
  onMounted,
  nextTick,
} from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useDiagnosticListeners } from "../composables/useDiagnosticListeners";
import DataTable from 'primevue/datatable';
import Column from 'primevue/column';
import Chart from 'primevue/chart';
import type { ChartData, ChartOptions } from "chart.js";
import type {
  TraceProtocol,
  TraceHop,
  TraceSetting,
  TraceDonePayload,
} from "../types/probe";
import type { Host } from "../types/net";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { fmtMs } from "../utils/formatter";

const form = reactive({
  protocol: "Icmp" as TraceProtocol,
  host: "1.1.1.1",
  max_hops: 30,
  tries_per_hop: 2,
  timeout_ms: 2000,
});

const running = ref(false);
const opId = ref<string | null>(null);
const run = useDiagnosticRun("traceroute", opId);
const canceling = ref(false);
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

let preparing = false;
let preparationCanceled = false;

async function startTrace() {
  if (preparing) return;
  if (!listenersReady.value) {
    err.value = "Diagnostics are not ready. Reopen this page if initialization failed.";
    return;
  }
  if (running.value || !form.host.trim()) return;
  resetResult();
  canceling.value = false;
  running.value = true;

  let runId: string | undefined;
  try {
    preparing = true;
    preparationCanceled = false;
    runId = await run.begin();
    const setting = await toTraceSetting();
    preparing = false;
    if (preparationCanceled || !listenersReady.value) return;
    await invoke("traceroute", { setting, runId });
  } catch (e: any) {
    if (runId && !run.isLatest(runId)) return;
    await run.cancel(runId).catch((error) => console.error("Failed to release diagnostic", error));
    if (!preparationCanceled) err.value = String(e?.message ?? e);
    running.value = false;
  } finally {
    if (runId && !run.isLatest(runId)) return;
    preparing = false;
  }
}

async function cancelTrace() {
  preparationCanceled = true;
  canceling.value = true;
  try {
    await run.cancel();
  } catch (error) {
    err.value = String(error);
  } finally {
    running.value = false;
    canceling.value = false;
  }
}

const { listen, dispose, listenersReady } = useDiagnosticListeners();

onMounted(async () => {
  try {
    await nextTick();

    // start
    await listen("traceroute:start", (ev:any) => {
      const p = ev?.payload ?? {};
      if (!run.accepts(p.run_id)) return;
    });

    // progress: each hop
    await listen("traceroute:progress", (ev: any) => {
      const hop: TraceHop | undefined = ev?.payload;
      if (!hop) return;
      if (!run.accepts(hop.run_id)) return;

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
    await listen("traceroute:done", (ev: any) => {
      const payload: TraceDonePayload | undefined = ev?.payload;
      if (!run.accepts(payload?.run_id)) return;
      if (payload) {
        doneInfo.value = payload;
      }
      running.value = false;
      canceling.value = false;
      run.finish();
    });

    // error
    await listen("traceroute:error", (ev: any) => {
      const p = ev?.payload ?? {};
      if (!run.accepts(p.run_id)) return;
      if (p.message) {
        err.value = String(p.message);
      }
      running.value = false;
      canceling.value = false;
      run.finish();
    });

    await listen("traceroute:cancelled", (ev: any) => {
      if (run.accepts(ev.payload?.run_id)) {
        run.finish();
        running.value = false;
        canceling.value = false;
      }
    });

    listenersReady.value = true;
  } catch (error) {
    dispose();
    err.value = `Could not initialize diagnostics: ${String(error)}. Reopen this page to retry.`;
  }
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

function fmtIp(ip?: string | null) {
  if (!ip) return "*";
  return ip;
}
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-5 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div
      ref="toolbarRef"
      class="nd-page-toolbar grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-3"
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
            size="small"
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
            size="small"
            @keydown.enter.prevent="startTrace"
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
            size="small"
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
            size="small"
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
            size="small"
          />
        </div>
      </div>

      <!-- Right: actions -->
      <div class="flex flex-wrap items-end gap-3 justify-end self-end">
        <Button
          label="Start"
          icon="pi pi-play"
          :disabled="running || !form.host?.trim()"
          :loading="running && !canceling"
          @click="startTrace"
          aria-label="Start traceroute"
          size="small"
        />
        <Button
          label="Cancel"
          icon="pi pi-stop"
          :disabled="!running || canceling"
          :loading="canceling"
          severity="secondary"
          size="small"
          @click="cancelTrace"
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
              <div v-if="err" class="mt-3 text-red-500 text-sm" role="alert">
                {{ err }}
              </div>
              <div class="text-xs text-surface-500 mt-3">
                {{ opId ? `Operation ID: ${opId}` : "" }}
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
