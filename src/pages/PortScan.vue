<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import DataTable from 'primevue/datatable';
import Column from 'primevue/column';
import {
  PortScanProtocol,
  PortScanReport,
  PortScanSample,
  PortScanSetting,
  ServiceInfo,
  TargetPortsPreset,
} from "../types/probe";
import { Host } from "../types/net";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { fmtMs } from "../utils/formatter";

const form = reactive({
  protocol: "Tcp" as PortScanProtocol,
  host: "",
  preset: "Common" as TargetPortsPreset,
  userPortsText: "80,443,8080,8443",
  timeout_ms: 1500,
  ordered: false,
  service_detection: false,
});

const activeRunId = ref<string | null>(null);
const running = ref(false);
const loading = ref(false);
const serviceDetecting = ref(false);
const err = ref<string | null>(null);

const progressDone = ref(0);
const progressTotal = ref(0);

const openOnly = ref<PortScanSample[]>([]);
const report = ref<PortScanReport | null>(null);

// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();

const targetPorts = ref<number[]>([]);
const targetCount = computed(() => targetPorts.value.length);

// Parse user-specified ports: "80,443,1000-1010"
function parseUserPorts(text: string): number[] {
  const out: number[] = [];
  for (const part of text
    .split(/[,\s]+/)
    .map((s) => s.trim())
    .filter(Boolean)) {
    if (/^\d+$/.test(part)) {
      const p = Number(part);
      if (p >= 1 && p <= 65535) out.push(p);
      continue;
    }
    const m = part.match(/^(\d+)-(\d+)$/);
    if (m) {
      let a = Number(m[1]),
        b = Number(m[2]);
      if (a > b) [a, b] = [b, a];
      for (let p = a; p <= b; p++) {
        if (p >= 1 && p <= 65535) out.push(p);
      }
    }
  }
  return Array.from(new Set(out)).sort((a, b) => a - b);
}

async function resolveTarget(target: string): Promise<Host> {
  const host: Host = await invoke("lookup_host", { host: target });
  return host;
}

async function refreshTargetPorts() {
  try {
    const userPorts = parseUserPorts(form.userPortsText);
    const ports = await invoke<number[]>("get_target_ports", {
      preset: form.preset,
      userPorts,
    });
    targetPorts.value = ports ?? [];
  } catch (e) {
    targetPorts.value = [];
    err.value = String(e);
  }
}

function serviceDetailText(s?: ServiceInfo | null): string {
  if (!s) return "-";

  const cpes = (s.cpes ?? []).filter(Boolean);
  if (cpes.length > 0) {
    const shown = cpes.slice(0, 3).join(", ");
    return cpes.length > 3 ? `${shown} (+${cpes.length - 3})` : shown;
  }

  const banner = (s.banner ?? "").trim();
  if (banner) return banner;

  const raw = (s.raw ?? "").trim();
  if (raw) return raw;

  const parts = [s.product, s.version, s.name].filter((v) => !!v && String(v).trim().length > 0);
  if (parts.length) return parts.join(" ");

  return "-";
}

let targetPortsTimer: number | null = null;

watch(
  () => [form.preset, form.userPortsText],
  () => {
    if (targetPortsTimer !== null) {
      clearTimeout(targetPortsTimer);
    }
    targetPortsTimer = window.setTimeout(() => {
      refreshTargetPorts();
    }, 300);
  },
  { immediate: true },
);

async function toSetting(): Promise<PortScanSetting> {
  const target = await resolveTarget(form.host);
  return {
    ip_addr: target.ip,
    hostname: target.hostname,
    target_ports_preset: form.preset,
    user_ports: parseUserPorts(form.userPortsText),
    protocol: form.protocol,
    timeout_ms: form.timeout_ms,
    ordered: form.ordered,
    service_detection: form.service_detection,
  };
}

function resetResult() {
  openOnly.value = [];
  report.value = null;
  err.value = null;
  progressDone.value = 0;
  progressTotal.value = 0;
}

const canStart = computed(() => !!form.host.trim());

async function startScan() {
  if (!canStart.value) return;
  resetResult();
  running.value = true;
  loading.value = true;

  try {
    const setting = await toSetting();
    const rep = await invoke<PortScanReport>("port_scan", { setting });
    report.value = rep;
    // Backend returns open-only samples in report.samples
    openOnly.value = rep.samples ?? [];
  } catch (e: any) {
    err.value = String(e?.message ?? e);
    running.value = false;
  } finally {
    loading.value = false;
  }
}

async function initProbeDb() {
  try {
    await invoke("init_probe_db");
  } catch (e) {
    console.error("Failed to init probe database:", e);
  }
}

const progressPct = computed(() => {
  const t = progressTotal.value || 0;
  const d = progressDone.value || 0;
  if (!t) return 0;
  return Math.min(100, Math.round((d / t) * 100));
});

const openCount = computed(() => openOnly.value.length);

let unlistenStart: UnlistenFn | null = null;
let unlistenProgress: UnlistenFn | null = null;
let unlistenOpen: UnlistenFn | null = null;
let unlistenDone: UnlistenFn | null = null;
let unlistenSvcStart: UnlistenFn | null = null;
let unlistenSvcDone: UnlistenFn | null = null;

// Set up event listeners on mount
onMounted(async () => {
  
  // Initialize probe database
  initProbeDb();

  // Start event
  unlistenStart = await listen("portscan:start", (ev:any) => {
    const runId: string | undefined = ev?.payload.run_id;
    if (runId) {
      activeRunId.value = runId;
    }
    progressDone.value = 0;
    progressTotal.value = 0;
  });
  // Progress event
  unlistenProgress = await listen("portscan:progress", (ev: any) => {
    const payload = ev?.payload;
    if (!payload) return;
    const [done, total] = payload as [number, number];
    progressDone.value = done;
    progressTotal.value = total;
  });
  // Open port sample event
  unlistenOpen = await listen("portscan:open", (ev: any) => {
    const s = ev?.payload as PortScanSample | undefined;
    if (!s) return;
    openOnly.value = [...openOnly.value, s];
  });

  unlistenSvcStart = await listen("portscan:service_detection_start", (ev: any) => {
    const runId = ev?.payload as string | undefined;
    if (activeRunId.value && runId && runId !== activeRunId.value) return;
    serviceDetecting.value = true;
  });

  unlistenSvcDone = await listen("portscan:service_detection_done", (ev: any) => {
    const runId = ev?.payload as string | undefined;
    if (activeRunId.value && runId && runId !== activeRunId.value) return;
    serviceDetecting.value = false;
  });

  // Done event
  unlistenDone = await listen("portscan:done", (ev: any) => {
    const rep: PortScanReport | undefined = ev?.payload;
    if (rep) {
      report.value = rep;
      openOnly.value = rep.samples ?? [];
    }
    running.value = false;
  });
});

// Clean up listeners on unmount
onBeforeUnmount(() => {
  unlistenStart?.();
  unlistenProgress?.();
  unlistenOpen?.();
  unlistenDone?.();
  unlistenSvcStart?.();
  unlistenSvcDone?.();
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
      class="grid grid-cols-1 lg:grid-cols-[1fr_auto] gap-3 items-center"
    >
      <!-- Left: form controls -->
      <div class="flex items-end gap-3 min-w-0 flex-wrap">
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Protocol</label>
          <Select
            v-model="form.protocol"
            :options="[
              { label: 'TCP', value: 'Tcp' },
              { label: 'QUIC', value: 'Quic' },
            ]"
            optionLabel="label"
            optionValue="value"
            class="min-w-[120px]"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Target Host / IP</label>
          <InputText
            v-model="form.host"
            placeholder="e.g. 192.168.1.1 or host"
            class="w-60"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Preset</label>
          <Select
            v-model="form.preset"
            :options="[
              { label: 'Common', value: 'Common' },
              { label: 'WellKnown', value: 'WellKnown' },
              { label: 'Top 1000', value: 'Top1000' },
              { label: 'Custom', value: 'Custom' },
            ]"
            optionLabel="label"
            optionValue="value"
            class="min-w-[140px]"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Extra Ports</label>
          <InputText
            v-model="form.userPortsText"
            placeholder="e.g. 80,443,8080-8090"
            class="w-[220px]"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Timeout (ms)</label>
          <InputNumber
            v-model="form.timeout_ms"
            :min="200"
            :max="10000"
            :step="100"
            inputClass="w-[120px]"
          />
        </div>

        <div class="flex items-center gap-2 mb-2">
          <Checkbox v-model="form.ordered" :binary="true" inputId="ordered" />
          <label for="ordered" class="text-sm">Ordered</label>
        </div>

        <div class="flex items-center gap-2 mb-2">
          <Checkbox v-model="form.service_detection" :binary="true" inputId="service_detection" v-tooltip.bottom="`Enable service detection.`" />
          <label for="service_detection" class="text-sm">Service</label>
        </div>

        <!-- Target count preview -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Targets</label>
          <Badge size="large" severity="secondary">{{ targetCount }}</Badge>
        </div>
      </div>

      <!-- Right: actions -->
      <div class="flex flex-wrap items-center gap-3 justify-end">
        <div class="flex items-center gap-2">
          <Button
            label="Start"
            icon="pi pi-play"
            :disabled="running || !canStart"
            :loading="loading"
            @click="startScan"
          />
        </div>
      </div>
    </div>

    <div class="flex-1 min-h-0">
      <!-- Scrollable content -->
      <ScrollPanel
        :style="{ width: '100%', height: panelHeight }"
        class="flex-1 min-h-0"
      >
        <div class="grid grid-cols-1 gap-3">
          <!-- Progress -->
          <Card>
            <template #title>Progress</template>
            <template #content>
              <div
                class="flex items-center justify-between mb-2 text-sm text-surface-500"
              >
                <div>Total: {{ progressTotal || "-" }}</div>
                <div>
                  Done: {{ progressDone }} / {{ progressTotal || "-" }}
                </div>
              </div>
              <ProgressBar :value="progressPct" />
              <div class="mt-2 text-xs text-surface-500">
                Open ports found: <span class="font-mono">{{ openCount }}</span>
              </div>
            </template>
          </Card>

          <!-- Results (Open only) -->
          <Card>
            <template #title>Results</template>
            <template #content>
              <div v-if="err" class="text-red-500 text-sm mb-2">
                {{ err }}
              </div>
              <div
                class="flex items-center justify-between mb-2 text-sm text-surface-500"
              >
                <div>Open: {{ openCount }}</div>
              </div>
              <div v-if="serviceDetecting" class="mb-2 text-xs text-surface-500 flex items-center gap-2">
                <i class="pi pi-spin pi-spinner"></i>
                <span>Service detection in progress...</span>
              </div>
              <div
                v-if="report"
                class="mt-1 text-xs text-surface-500"
              >
                Completed {{ report.protocol.toUpperCase() }} scan for
                <span
                  v-if="report.hostname"
                  class="font-mono"
                >{{ `${report.hostname} (${report.ip_addr})` }}</span>
                <span
                  v-else
                  class="font-mono"
                >{{ `${report.ip_addr}` }}</span>
              </div>

              <template v-if="openOnly.length">
                <div class="mt-3">
                  <DataTable
                    :value="openOnly"
                    size="small"
                    stripedRows
                    class="text-sm copyable"
                    :rows="10"
                    paginator
                    :rowsPerPageOptions="[10, 20, 50]"
                    sortMode="single"
                    sortField="port"
                    :sortOrder="1"
                  >
                    <Column
                      field="port"
                      header="Port"
                      style="width: 96px"
                      sortable
                    />
                    <Column
                      field="service_name"
                      header="Service"
                      sortable
                    >
                      <template #body="{ data }">
                        <span class="font-mono">{{
                          data.service_name || "-"
                        }}</span>
                      </template>
                    </Column>
                    <Column
                      field="rtt_ms"
                      header="RTT"
                      sortable
                    >
                      <template #body="{ data }">
                        {{ fmtMs(data.rtt_ms) }}
                      </template>
                    </Column>
                    <Column header="Service Detail">
                      <template #body="{ data }">
                        <span class="font-mono text-xs text-surface-600 dark:text-surface-300">
                          {{ serviceDetailText(data.service_info) }}
                        </span>
                      </template>
                    </Column>
                  </DataTable>
                </div>
              </template>

              <template v-else>
                <div class="text-surface-500 text-sm">
                  No open ports found yet.
                </div>
              </template>
            </template>
          </Card>
        </div>
      </ScrollPanel>
    </div>
  </div>
</template>
