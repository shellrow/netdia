<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import {
  HostScanProgress,
  HostScanProgressPayload,
  NeighborScanReport,
  NeighborScanStartPayload,
  NeighborScanCancelledPayload,
  NeighborScanErrorPayload,
} from "../types/probe";
import { Ipv4Net } from "../types/net";
import { fmtMs } from "../utils/formatter";

const activeRunId = ref<string | null>(null);
const running = ref(false);
const loading = ref(false);
const canceling = ref(false);
const cancelled = ref(false);
const err = ref<string | null>(null);

const report = ref<NeighborScanReport | null>(null);

const progressDone = ref(0);
const progressTotal = ref(0);
const foundAlive = ref(0);

// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight({
  extra: 28,
  gap: 12,
  min: 220,
});

const netMap = ref<Record<string, Ipv4Net>>({});
const selectedIf = ref<string | null>(null);

const ifOptions = computed(() =>
  Object.entries(netMap.value).map(([name, net]) => {
    let cidr = "-";
    if (typeof net === "string") cidr = net;
    else if (net && typeof net === "object") cidr = `${net.addr}/${net.prefix_len}`;
    return { label: `${name}  ${cidr}`, value: name };
  }),
);

const selectedCidr = computed(() => {
  const name = selectedIf.value;
  if (!name) return "-";
  const net = netMap.value[name];
  if (!net) return "-";
  return typeof net === "string" ? net : `${net.addr}/${net.prefix_len}`;
});

function resetAll() {
  err.value = null;
  report.value = null;
  progressDone.value = 0;
  progressTotal.value = 0;
  foundAlive.value = 0;
  cancelled.value = false;
  activeRunId.value = null;
}

const progressPct = computed(() => {
  const t = progressTotal.value || 0;
  const d = progressDone.value || 0;
  if (!t) return 0;
  return Math.min(100, Math.round((d / t) * 100));
});

const neighborCount = computed(() => report.value?.neighbors?.length ?? 0);

async function fetchNetworkAddressMap() {
  try {
    const m = await invoke<Record<string, Ipv4Net>>("get_network_address_map");
    netMap.value = m ?? {};
    const first = Object.keys(netMap.value)[0];
    selectedIf.value = first ?? null;
  } catch (e: any) {
    err.value = `failed to load networks: ${String(e?.message ?? e)}`;
  }
}

async function startScan() {
  resetAll();
  running.value = true;
  loading.value = true;

  try {
    await invoke("neighbor_scan", { ifaceName: selectedIf.value ?? null });
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
    await invoke("cancel_neighborscan");
  } catch (e: any) {
    err.value = String(e?.message ?? e);
  } finally {
    canceling.value = false;
  }
}

let unlistenHostProgress: UnlistenFn | null = null;
let unlistenHostAlive: UnlistenFn | null = null;
let unlistenNeighborStart: UnlistenFn | null = null;
let unlistenNeighborDone: UnlistenFn | null = null;
let unlistenNeighborCancelled: UnlistenFn | null = null;
let unlistenNeighborError: UnlistenFn | null = null;

onMounted(async () => {
  await nextTick();

  unlistenNeighborStart = await listen<NeighborScanStartPayload>("neighborscan:start", (ev) => {
    const runId = ev?.payload?.run_id;
    if (runId) activeRunId.value = runId;
    running.value = true;
    err.value = null;
    cancelled.value = false;
    progressDone.value = 0;
    progressTotal.value = 0;
    foundAlive.value = 0;
  });

  unlistenHostProgress = await listen<HostScanProgressPayload>("hostscan:progress", (ev) => {
    const p = ev?.payload;
    if (!p) return;
    if (activeRunId.value && p.run_id && p.run_id !== activeRunId.value) return;
    progressDone.value = p.done;
    progressTotal.value = p.total;
  });
  
  unlistenHostAlive = await listen<HostScanProgress>("hostscan:alive", (ev) => {
    const p = ev?.payload;
    if (!p) return;
    if (activeRunId.value && p.run_id && p.run_id !== activeRunId.value) return;
    foundAlive.value += 1;
  });

  unlistenNeighborDone = await listen<NeighborScanReport>("neighborscan:done", (ev) => {
    const rep = ev?.payload;
    if (!rep) return;
    if (activeRunId.value && rep.run_id && rep.run_id !== activeRunId.value) return;
    report.value = rep;
    running.value = false;
    canceling.value = false;
  });

  unlistenNeighborCancelled = await listen<NeighborScanCancelledPayload>("neighborscan:cancelled", (ev) => {
    const p = ev?.payload;
    const runId = p?.run_id;
    if (activeRunId.value && runId && runId !== activeRunId.value) return;
    cancelled.value = true;
    running.value = false;
    canceling.value = false;
    loading.value = false;
  });

  unlistenNeighborError = await listen<NeighborScanErrorPayload>("neighborscan:error", (ev) => {
    const p = ev?.payload;
    const runId = p?.run_id;
    if (activeRunId.value && runId && runId !== activeRunId.value) return;
    err.value = String(p?.message ?? "neighborscan error");
    running.value = false;
    canceling.value = false;
    loading.value = false;
  });

  await fetchNetworkAddressMap();
});

onBeforeUnmount(() => {
  unlistenHostProgress?.();
  unlistenHostAlive?.();
  unlistenNeighborStart?.();
  unlistenNeighborDone?.();
  unlistenNeighborCancelled?.();
  unlistenNeighborError?.();
});
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-start gap-3">
      <div class="flex items-center gap-3 min-w-0 flex-wrap">
        <div class="text-surface-500 dark:text-surface-400 text-sm">Neighbor Scan</div>

        <Select
          v-model="selectedIf"
          :options="ifOptions"
          optionLabel="label"
          optionValue="value"
          placeholder="Select interface"
          class="min-w-64"
          :disabled="running || loading"
          size="small"
        />
        <Tag v-if="selectedIf" :value="selectedCidr" class="font-mono" severity="info"/>
        <span v-else class="text-surface-500 text-xs">No eligible interface</span>
      </div>

      <div class="flex items-center justify-end gap-2">
        <Button
          label="Start"
          icon="pi pi-play"
          :disabled="running || !selectedIf"
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
          <Card>
            <template #title>Progress</template>
            <template #content>
              <div class="flex items-center justify-between mb-2 text-sm text-surface-500">
                <div>Scanned: {{ progressDone }} / {{ progressTotal || "-" }}</div>
                <div>{{ progressPct }}%</div>
              </div>
              <ProgressBar :value="progressPct" />
              <div class="mt-2 text-xs text-surface-500">
                Alive hosts found: <span class="font-mono">{{ foundAlive }}</span>
                <span v-if="cancelled" class="ml-2 text-orange-500">(cancelled)</span>
              </div>
            </template>
          </Card>

          <Card>
            <template #title>Neighbors</template>
            <template #content>
              <div v-if="err" class="text-red-500 text-sm mb-2">{{ err }}</div>

              <template v-if="report">
                <div class="grid grid-cols-2 gap-3 text-sm mb-3">
                  <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">Total Alive</div>
                    <div class="font-medium">{{ neighborCount }}</div>
                  </div>
                  <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">Scanned</div>
                    <div class="font-medium">{{ report.total }}</div>
                  </div>
                </div>

                <DataTable
                  :value="report.neighbors"
                  size="small"
                  stripedRows
                  class="text-sm copyable"
                  :rows="10"
                  paginator
                  :rowsPerPageOptions="[10, 20, 50]"
                  sortMode="single"
                >
                  <Column field="ip_addr" header="IP" sortable />
                  <Column field="mac_addr" header="MAC" sortable>
                    <template #body="{ data }">
                      <span class="font-mono">{{ data.mac_addr ?? "-" }}</span>
                    </template>
                  </Column>
                  <Column field="vendor" header="Vendor" sortable>
                    <template #body="{ data }">
                      <span>{{ data.vendor ?? "-" }}</span>
                    </template>
                  </Column>
                  <Column field="rtt_ms" header="RTT" sortable>
                    <template #body="{ data }">{{ fmtMs(data.rtt_ms) }}</template>
                  </Column>
                  <Column field="tags" header="Tags" sortable>
                    <template #body="{ data }">
                      <div class="flex flex-wrap gap-1">
                        <Tag
                          v-for="t in (data.tags || [])"
                          :key="t"
                          :value="t"
                          :severity="t === 'Gateway' ? 'warn' : (t === 'Self' ? 'info' : (t === 'DNS' ? 'secondary' : 'contrast'))"
                          class="text-xs"
                        />
                        <span v-if="!data.tags || data.tags.length === 0" class="text-surface-500">-</span>
                      </div>
                    </template>
                  </Column>
                </DataTable>
              </template>

              <template v-else>
                <div class="text-surface-500 text-sm">Press Start to run a neighbor scan.</div>
              </template>
            </template>
          </Card>
        </div>
      </ScrollPanel>
    </div>
  </div>
</template>
