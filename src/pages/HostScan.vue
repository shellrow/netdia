<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount, watch } from "vue";
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
  HostScanTargetPreview,
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

const targetPreview = ref<HostScanTargetPreview>({
  targets: [],
  estimated_count: 0,
  exceeds_limit: false,
});

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
  targetPreview.value.estimated_count,
);

const canStart = computed(
  () =>
    targetCount.value > 0 &&
    !targetPreview.value.exceeds_limit &&
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

async function refreshTargetPreview() {
  targetPreview.value = await invoke<HostScanTargetPreview>(
    "preview_host_scan_targets",
    {
      mode: form.mode,
      cidr: form.cidr,
      list: form.list,
      maxExpand: MAX_EXPAND,
    },
  );
}

async function startScan() {
  resetResult();

  const preview = await invoke<HostScanTargetPreview>("preview_host_scan_targets", {
    mode: form.mode,
    cidr: form.cidr,
    list: form.list,
    maxExpand: MAX_EXPAND,
  });
  targetPreview.value = preview;
  const targets = preview.targets;
  if (targets.length === 0) {
    err.value =
      preview.exceeds_limit
        ? `Target too large (${preview.estimated_count} hosts). Please use a narrower CIDR or increase the limit.`
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
  await refreshTargetPreview();
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

watch(
  () => [form.mode, form.cidr, form.list],
  () => {
    void refreshTargetPreview();
  },
  { immediate: false },
);

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
