<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import Textarea from "primevue/textarea";
import {
  HostScanProgress,
  HostScanReport,
  HostScanRequest,
  HostScanStartPayload,
  HostScanCancelledPayload,
  HostScanErrorPayload,
  HostScanProgressPayload,
} from "../types/probe";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { fmtMs } from "../utils/formatter";

const form = reactive({
  mode: "cidr" as "cidr" | "list",
  cidr: "",
  list: "192.168.1.1\n192.168.1.2",
  hop_limit: 64,
  timeout_ms: 1000,
  count: 1,
  payload: "netd",
  ordered: false,
  concurrency: 100,
});

const activeRunId = ref<string | null>(null);
const running = ref(false);
const loading = ref(false);
const canceling = ref(false);
const cancelled = ref(false);
const err = ref<string | null>(null);

const progressDone = ref(0);
const progressTotal = ref(0);

type AliveRow = { ip: string; hostname?: string | null; rtt: number | null };
const aliveRows = ref<AliveRow[]>([]);
const report = ref<HostScanReport | null>(null);

// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();

const MAX_EXPAND = 65536;

// Expand IPv4 CIDR "192.168.1.0/24" to host list (excludes network/broadcast when prefix <= 30)
function expandIpv4Cidr(cidr: string, max = MAX_EXPAND): string[] {
  const m = cidr.trim().match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)\/(\d{1,2})$/);
  if (!m) return [];
  const [, a, b, c, d, prefixStr] = m;
  const prefix = parseInt(prefixStr, 10);
  if (prefix < 0 || prefix > 32) return [];

  const total = estimateHosts(cidr);
  if (total === 0 || total > max) return [];

  const ipNum =
    (parseInt(a, 10) << 24) |
    (parseInt(b, 10) << 16) |
    (parseInt(c, 10) << 8) |
    parseInt(d, 10);
  const mask = prefix === 0 ? 0 : (~0 << (32 - prefix)) >>> 0;
  const net = ipNum & mask;
  const size = 2 ** (32 - prefix);
  const start = prefix <= 30 ? 1 : 0;
  const end = prefix <= 30 ? size - 2 : size - 1;

  const ips: string[] = [];
  for (let i = start; i <= end; i++) {
    const val = (net + i) >>> 0;
    const A = (val >>> 24) & 0xff;
    const B = (val >>> 16) & 0xff;
    const C = (val >>> 8) & 0xff;
    const D = val & 0xff;
    ips.push(`${A}.${B}.${C}.${D}`);
  }
  return ips;
}

function estimateHosts(cidr: string): number {
  const m = cidr.trim().match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)\/(\d{1,2})$/);
  if (!m) return 0;
  const prefix = parseInt(m[5], 10);
  if (prefix < 0 || prefix > 32) return 0;
  const size = 2 ** (32 - prefix);
  const usable = prefix <= 30 ? Math.max(0, size - 2) : size;
  return usable;
}

function resetResult() {
  progressDone.value = 0;
  progressTotal.value = 0;
  aliveRows.value = [];
  report.value = null;
  err.value = null;
  cancelled.value = false;
  activeRunId.value = null;
}

const targetCount = computed(() =>
  form.mode === "cidr"
    ? estimateHosts(form.cidr)
    : new Set(
        (form.list || "")
          .split(/[\s,;]+/)
          .map((s) => s.trim())
          .filter(Boolean),
      ).size,
);

const canStart = computed(
  () =>
    targetCount.value > 0 &&
    (form.mode !== "cidr" || targetCount.value <= MAX_EXPAND) &&
    !loading.value &&
    !running.value,
);

const progressPct = computed(() => {
  const t = progressTotal.value || 0;
  const d = progressDone.value || 0;
  if (!t) return 0;
  return Math.min(100, Math.round((d / t) * 100));
});

const aliveCount = computed(() => aliveRows.value.length);
const unreachableCount = computed(() => {
  if (!report.value) return 0;
  return report.value.unreachable.length;
});

function parseTargetsForStart(): string[] {
  if (form.mode === "cidr") {
    return expandIpv4Cidr(form.cidr, MAX_EXPAND);
  }
  const raw = form.list || "";
  const tokens = raw
    .split(/[\s,;]+/)
    .map((s) => s.trim())
    .filter(Boolean);
  return Array.from(new Set(tokens));
}

async function startScan() {
  resetResult();

  const targets = parseTargetsForStart();
  if (targets.length === 0) {
    const est = form.mode === "cidr" ? estimateHosts(form.cidr) : 0;
    err.value =
      est > MAX_EXPAND
        ? `Target too large (${est} hosts). Please use a narrower CIDR or increase the limit.`
        : "No targets. Add CIDR or IP list.";
    return;
  }

  running.value = true;
  loading.value = true;

  const setting: HostScanRequest = {
    targets,
    hop_limit: form.hop_limit,
    timeout_ms: form.timeout_ms,
    count: form.count,
    payload: form.payload || null,
    ordered: form.ordered,
    concurrency: form.concurrency || null,
  };

  try {
    await invoke("host_scan", { setting });
  } catch (e: any) {
    err.value = String(e?.message ?? e);
    running.value = false;
  } finally {
    loading.value = false;
  }
}

async function cancelScan() {
  canceling.value = true;
  try {
    await invoke("cancel_hostscan");
  } catch (e: any) {
    err.value = String(e?.message ?? e);
  } finally {
    canceling.value = false;
  }
}

let unlistenStart: UnlistenFn | null = null;
let unlistenProgress: UnlistenFn | null = null;
let unlistenAlive: UnlistenFn | null = null;
let unlistenDone: UnlistenFn | null = null;
let unlistenCancelled: UnlistenFn | null = null;
let unlistenError: UnlistenFn | null = null;

onMounted(async () => {
  unlistenStart = await listen<HostScanStartPayload>("hostscan:start", (ev) => {
    const runId = ev?.payload?.run_id;
    if (runId) activeRunId.value = runId;
    progressDone.value = 0;
    progressTotal.value = 0;
  });

  unlistenProgress = await listen<HostScanProgressPayload>("hostscan:progress", (ev) => {
    const p = ev?.payload;
    if (!p) return;
    if (activeRunId.value && p.run_id && p.run_id !== activeRunId.value) return;
    progressDone.value = p.done;
    progressTotal.value = p.total;
  });

  unlistenAlive = await listen<HostScanProgress>("hostscan:alive", (ev) => {
    const p = ev?.payload;
    if (!p) return;
    if (activeRunId.value && p.run_id && p.run_id !== activeRunId.value) return;

    aliveRows.value = [
      ...aliveRows.value,
      {
        ip: String(p.ip_addr),
        rtt: p.rtt_ms ?? null,
      },
    ];
  });

  unlistenDone = await listen<HostScanReport>("hostscan:done", (ev) => {
    const rep = ev?.payload;
    if (!rep) return;
    if (activeRunId.value && rep.run_id && rep.run_id !== activeRunId.value) return;

    report.value = rep;
    aliveRows.value = rep.alive.map(([host, rtt]) => ({
      ip: String(host.ip),
      hostname: host.hostname,
      rtt,
    }));

    running.value = false;
    loading.value = false;
    canceling.value = false;
  });

  unlistenCancelled = await listen<HostScanCancelledPayload>("hostscan:cancelled", (ev) => {
    const p = ev?.payload;
    const runId = p?.run_id;
    if (activeRunId.value && runId && runId !== activeRunId.value) return;

    cancelled.value = true;
    running.value = false;
    loading.value = false;
    canceling.value = false;
  });

  unlistenError = await listen<HostScanErrorPayload>("hostscan:error", (ev) => {
    const p = ev?.payload;
    const runId = p?.run_id;
    if (activeRunId.value && runId && runId !== activeRunId.value) return;

    err.value = String(p?.message ?? "hostscan error");
    running.value = false;
    loading.value = false;
    canceling.value = false;
  });
});

onBeforeUnmount(() => {
  unlistenStart?.();
  unlistenProgress?.();
  unlistenAlive?.();
  unlistenDone?.();
  unlistenCancelled?.();
  unlistenError?.();
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
      <div class="flex items-end gap-3 min-w-0 flex-wrap">
        <!-- Mode -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Mode</label>
          <Select
            v-model="form.mode"
            :options="[
              { label: 'CIDR (IPv4)', value: 'cidr' },
              { label: 'List (Hosts)', value: 'list' },
            ]"
            optionLabel="label"
            optionValue="value"
            class="min-w-40"
            size="small"
          />
        </div>

        <!-- CIDR / List -->
        <div v-if="form.mode === 'cidr'" class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">CIDR</label>
          <InputText
            v-model="form.cidr"
            placeholder="e.g. 192.168.1.0/24"
            class="w-[220px]"
            size="small"
          />
        </div>
        <div v-else class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Host List (newline / space / comma)</label>
          <Textarea v-model="form.list" rows="2" class="w-[280px]" size="small" />
        </div>

        <!-- Options -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Timeout (ms)</label>
          <InputNumber
            v-model="form.timeout_ms"
            :min="100"
            :max="60000"
            :step="100"
            inputClass="w-[120px]"
            size="small"
          />
        </div>
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">TTL / Hop Limit</label>
          <InputNumber
            v-model="form.hop_limit"
            :min="1"
            :max="255"
            inputClass="w-[120px]"
            size="small"
          />
        </div>

        <div class="flex items-center gap-2 mb-2">
          <Checkbox v-model="form.ordered" :binary="true" inputId="ordered" />
          <label for="ordered" class="text-sm">Ordered</label>
        </div>

        <!-- Target count preview -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Targets</label>
          <Badge size="large" severity="secondary">{{ targetCount }}</Badge>
        </div>
      </div>

      <div class="flex flex-wrap items-end gap-3 justify-end self-end">
        <Button
          label="Start"
          icon="pi pi-play"
          :disabled="!canStart"
          :loading="loading"
          @click="startScan"
          size="small"
        />
        <Button
          label="Cancel"
          icon="pi pi-stop"
          severity="secondary"
          :disabled="!running"
          :loading="canceling"
          @click="cancelScan"
          size="small"
        />
      </div>
    </div>

    <div class="flex-1 min-h-0">
      <ScrollPanel :style="{ width: '100%', height: panelHeight }" class="flex-1 min-h-0">
        <div class="grid grid-cols-1 gap-3">
          <!-- Progress -->
          <Card>
            <template #title>Progress</template>
            <template #content>
              <div class="flex items-center justify-between mb-2 text-sm text-surface-500">
                <div>Scanned: {{ progressDone }} / {{ progressTotal || "-" }}</div>
                <div>{{ progressPct }}%</div>
              </div>
              <ProgressBar :value="progressPct" />
              <div class="mt-2 text-xs text-surface-500">
                Alive hosts found: <span class="font-mono">{{ aliveCount }}</span>
                <span v-if="cancelled" class="ml-2 text-orange-500">(cancelled)</span>
              </div>
            </template>
          </Card>

          <!-- Summary -->
          <Card>
            <template #title>Summary</template>
            <template #content>
              <div v-if="err" class="text-red-500 text-sm mb-2">
                {{ err }}
              </div>

              <div class="grid grid-cols-2 gap-3 text-sm mb-3">
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Alive</div>
                  <div class="font-medium">{{ aliveCount }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Unreachable</div>
                  <div class="font-medium">{{ unreachableCount }}</div>
                </div>
              </div>

              <template v-if="aliveRows.length">
                <div class="font-semibold mb-1 text-sm">Alive Hosts</div>
                <DataTable
                  :value="aliveRows"
                  size="small"
                  stripedRows
                  class="text-sm copyable"
                  :rows="10"
                  paginator
                  :rowsPerPageOptions="[10, 20, 50]"
                  sortMode="single"
                  sortField="ip"
                  :sortOrder="1"
                >
                  <Column field="ip" header="IP" sortable />
                  <Column field="hostname" header="Hostname" sortable />
                  <Column field="rtt" header="RTT" sortable>
                    <template #body="{ data }">
                      {{ fmtMs(data.rtt) }}
                    </template>
                  </Column>
                </DataTable>
              </template>

              <template v-else>
                <div class="text-surface-500 text-sm">
                  Run a scan to see alive hosts.
                </div>
              </template>
            </template>
          </Card>
        </div>
      </ScrollPanel>
    </div>
  </div>
</template>
