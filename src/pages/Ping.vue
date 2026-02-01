<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import DataTable from 'primevue/datatable';
import Column from 'primevue/column';
import { PingProtocol, PingSample, PingStat, PingSetting } from "../types/probe";
import { Host } from "../types/net";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { fmtMs } from "../utils/formatter";

const form = reactive({
  protocol: "Icmp" as PingProtocol,
  host: "1.1.1.1",
  port: 443,
  count: 4,
  timeout_ms: 2000,
  send_rate_ms: 1000,
  hop_limit: 64,
});

const running = ref(false);
const opId = ref<string | null>(null);
const canceling = ref(false);
const err = ref<string | null>(null);

const samples = ref<PingSample[]>([]);
const stat = ref<PingStat | null>(null);
// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();

function resetResult() {
  // Clear previous run artifacts
  samples.value = [];
  stat.value = null;
  err.value = null;
  opId.value = null;
}

async function resolveTarget(target: string): Promise<Host> {
  const host: Host = await invoke("lookup_host", { host: target });
  return host;
}

const needsPort = computed(() => ["Tcp", "Udp", "Quic", "Http"].includes(form.protocol));

async function toPingSetting(): Promise<PingSetting> {
  const target = await resolveTarget(form.host);
  return {
    hostname: target.hostname,
    ip_addr: target.ip,
    port: needsPort.value ? form.port : null,
    hop_limit: form.hop_limit,
    protocol: form.protocol,
    count: form.count,
    timeout_ms: form.timeout_ms,
    send_rate_ms: form.send_rate_ms,
  };
}

async function startPing() {
  resetResult();
  canceling.value = false;
  running.value = true;

  try {
    const setting = await toPingSetting();
    await invoke("ping", { setting });
  } catch (e: any) {
    err.value = String(e?.message ?? e);
    running.value = false;
  }
}

async function cancelPing() {
  if (!running.value) return;
  canceling.value = true;
  try {
    await invoke('cancel_ping');
  } catch (e: any) {
    err.value = String(e?.message ?? e);
    canceling.value = false;
  }
}

let unlistenStart: UnlistenFn | null = null;
let unlistenProgress: UnlistenFn | null = null;
let unlistenDone: UnlistenFn | null = null;
let unlistenError: UnlistenFn | null = null;
let unlistenCancelled: UnlistenFn | null = null;

onMounted(async () => {
  await nextTick();

  // Ping started
  unlistenStart = await listen("ping:start", (ev: any) => {
    const p = ev?.payload ?? {};
    opId.value = p.run_id ?? null;
  });

  // Progress samples
  unlistenProgress = await listen("ping:progress", (ev: any) => {
    const p = ev?.payload ?? {};
    if (opId.value && p.run_id && p.run_id !== opId.value) return;

    const sample: PingSample | undefined = p.sample ?? ev?.payload;
    if (!sample) return;
    samples.value = [...samples.value, sample];
  });

  // Done (final stats)
  unlistenDone = await listen("ping:done", (ev: any) => {
    const p = ev?.payload ?? {};
    if (opId.value && p.run_id && p.run_id !== opId.value) return;

    const s: PingStat | undefined = p.stat ?? ev?.payload;
    if (s) stat.value = s;
    running.value = false;
  });

  unlistenError = await listen("ping:error", (ev:any) => {
    const p = ev?.payload ?? {};
    if (p.message) {
      err.value = String(p.message);
    }
    running.value = false;
    canceling.value = false;
  });

  unlistenCancelled = await listen("ping:cancelled", (ev: any) => {
    if (ev.payload?.run_id === opId.value) {
      running.value = false;
      canceling.value = false;
    }
  });
});

onBeforeUnmount(() => {
  unlistenStart?.();
  unlistenProgress?.();
  unlistenDone?.();
  unlistenError?.();
  unlistenCancelled?.();
});

const sentCount = computed(() => samples.value.length);
const recvCount = computed(() =>
  samples.value.filter(s => s.rtt_ms != null && s.probe_status.kind === "Done").length
);
const lossRate = computed(() => {
  if (sentCount.value === 0) return 0;
  return Math.max(0, Math.round((1 - recvCount.value / sentCount.value) * 100));
});
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-3">
      <!-- Left: filters -->
      <div class="flex flex-wrap items-end gap-3 min-w-0">
        <!-- Protocol -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Protocol</label>
          <Select
            v-model="form.protocol"
            :options="[
              { label: 'ICMP', value: 'Icmp' },
              { label: 'TCP',  value: 'Tcp'  },
              { label: 'UDP',  value: 'Udp'  },
              { label: 'QUIC', value: 'Quic' },
              { label: 'HTTP', value: 'Http' },
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
            class="w-[200px]"
            aria-label="Host or IP address"
            size="small"
          />
        </div>

        <!-- Port (conditional) -->
        <div class="flex flex-col gap-1" v-if="needsPort">
          <label class="text-xs text-surface-500">Port</label>
          <InputNumber
            v-model="form.port"
            :min="1" :max="65535"
            inputClass="w-[120px]"
            aria-label="Port" 
            size="small"
          />
        </div>

        <!-- Count -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Count</label>
          <InputNumber
            v-model="form.count"
            :min="1" :max="1000"
            inputClass="w-[110px]"
            aria-label="Packet count" 
            size="small"
          />
        </div>

        <!-- Timeout -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Timeout (ms)</label>
          <InputNumber
            v-model="form.timeout_ms"
            :min="100" :max="60000" :step="100"
            inputClass="w-[130px]"
            aria-label="Timeout in milliseconds"
            size="small"
          />
        </div>

        <!-- Interval -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Interval (ms)</label>
          <InputNumber
            v-model="form.send_rate_ms"
            :min="100" :max="60000" :step="10"
            inputClass="w-[130px]"
            aria-label="Send interval in milliseconds"
            size="small"
          />
        </div>

        <!-- TTL / Hop Limit -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">TTL / Hop Limit</label>
          <InputNumber
            v-model="form.hop_limit"
            :min="1" :max="255"
            inputClass="w-[120px]"
            aria-label="Hop limit"
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
          @click="startPing"
          aria-label="Start ping"
          size="small"
        />
        <Button
          :disabled="!running"
          :loading="canceling"
          label="Cancel"
          icon="pi pi-stop"
          severity="secondary"
          size="small"
          @click="cancelPing"
        />
      </div>
    </div>

    <div class="flex-1 min-h-0">
    <!-- Scrollable content -->
    <ScrollPanel :style="{ width: '100%', height: panelHeight }" class="flex-1 min-h-0">
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-3">
        <!-- Progress / Samples -->
        <Card>
          <template #title>Progress</template>
          <template #content>
            <div class="flex items-center justify-between mb-2">
              <div class="text-sm text-surface-500">Sent: {{ sentCount }} / {{ form.count }}</div>
              <div class="text-sm text-surface-500">Recv: {{ recvCount }} (loss {{ lossRate }}%)</div>
            </div>
            <ProgressBar :value="Math.min(100, Math.round((sentCount / form.count) * 100))" />
            <div class="mt-3">
              <DataTable
                :value="samples"
                size="small"
                stripedRows
                class="text-sm copyable"
                :rows="10"
                paginator
                :rowsPerPageOptions="[10,20,50]"
              >
                <Column field="seq" header="#" style="width: 72px" />
                <Column header="Status" style="width: 120px">
                  <template #body="{ data }">
                    <Tag
                      :value="data.probe_status.kind"
                      :severity="data.probe_status.kind === 'Done' ? 'success' : data.probe_status.kind === 'Timeout' ? 'warn' : 'danger'"
                    />
                  </template>
                </Column>
                <Column header="RTT">
                  <template #body="{ data }">{{ fmtMs(data.rtt_ms) }}</template>
                </Column>
                <Column header="Message">
                  <template #body="{ data }">
                    <span class="text-surface-500" v-if="data.probe_status.message">{{ data.probe_status.message }}</span>
                    <span v-else>-</span>
                  </template>
                </Column>
              </DataTable>
            </div>
          </template>
        </Card>

        <!-- Summary -->
        <Card>
          <template #title>Summary</template>
          <template #content>
            <div v-if="err" class="text-red-500 text-sm" aria-live="polite">{{ err }}</div>
            <template v-else>
              <div class="grid grid-cols-2 gap-3 text-sm">
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Protocol</div>
                  <div class="font-medium">{{ stat?.protocol.toUpperCase() ?? form.protocol.toUpperCase() }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Target</div>
                  <div class="font-mono break-all copyable">
                    {{ stat?.hostname ? `${stat.hostname} (${stat.ip_addr})` : form.host }}
                  </div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Sent / Recv</div>
                  <div class="font-medium">
                    {{ stat?.transmitted_count ?? sentCount }} / {{ stat?.received_count ?? recvCount }}
                  </div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Loss</div>
                  <div class="font-medium">{{ lossRate }}%</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Min</div>
                  <div class="font-medium">{{ fmtMs(stat?.min ?? null) }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Avg</div>
                  <div class="font-medium">{{ fmtMs(stat?.avg ?? null) }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Max</div>
                  <div class="font-medium">{{ fmtMs(stat?.max ?? null) }}</div>
                </div>
              </div>
              <div class="text-xs text-surface-500 mt-3">
                {{ opId ? `Operation ID: ${opId}` : "" }}
              </div>
            </template>
          </template>
        </Card>
      </div>
    </ScrollPanel>
    </div>
  </div>
</template>
