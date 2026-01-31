<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { IpInfo, IpInfoDual } from "../types/internet";
import { fmtBytes, nv } from "../utils/formatter";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { usePrivacyGate } from "../composables/usePrivacyGate";

// Public IP Info
const loading = ref(false);
const ipv4 = ref<IpInfo | null>(null);
const ipv6 = ref<IpInfo | null>(null);
const { publicIpVisible, togglePublicIp, pubIpGate } = usePrivacyGate();

async function refresh() {
  loading.value = true;
  try {
    const data = (await invoke("get_public_ip_info")) as IpInfoDual;
    ipv4.value = (data.ipv4 ?? null) as IpInfo | null;
    ipv6.value = (data.ipv6 ?? null) as IpInfo | null;
  } finally {
    loading.value = false;
  }
}

// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();

// Speed Test
type Direction = "download" | "upload";
type Result = "full" | "timeout" | "canceled" | "error";

const speedDirection = ref<Direction>("download");

const sizeOptions = ref([
  { label: "100 KB", bytes: 100 * 1024 },
  { label: "1 MB", bytes: 1 * 1024 * 1024 },
  { label: "10 MB", bytes: 10 * 1024 * 1024 },
  { label: "25 MB", bytes: 25 * 1024 * 1024 },
  { label: "50 MB", bytes: 50 * 1024 * 1024 },
  { label: "100 MB", bytes: 100 * 1024 * 1024 },
]);
type SizeOption = (typeof sizeOptions.value)[number];
// default 10MB
const selectedSize = ref<SizeOption>(sizeOptions.value[2]);

const latencyRunning = ref(false);
const latencyMs = ref<number | null>(null);
const jitterMs = ref<number | null>(null);
const edgeColo = ref<string | null>(null);

const stStarting = ref(false);
const stRunning = ref(false);
const stElapsedMs = ref(0);
const stTransferred = ref(0);
const stTarget = ref(0);
const stInstantMbps = ref(0);
const stAvgMbps = ref(0);
const stMaxMbps = ref(0);
const stDoneResult = ref<Result | null>(null);
const stMessage = ref<string | null>(null);

const progressPct = computed(() => {
  const target = stTarget.value || selectedSize.value.bytes || 0;
  if (target <= 0) return 0;

  const ratio = (stTransferred.value || 0) / target;
  const pct = ratio * 100;

  if (!Number.isFinite(pct)) return 0;

  return Math.min(100, Math.max(0, Math.round(pct * 10) / 10));
});

const transferredText = computed(() => fmtBytes(stTransferred.value));
const targetText = computed(() => fmtBytes(stTarget.value || selectedSize.value.bytes));
const elapsedText = computed(() => fmtDuration(stElapsedMs.value));

const maxDurationMs = 30_000;

function resetSpeedtestUi() {
  stElapsedMs.value = 0;
  stTransferred.value = 0;
  stTarget.value = selectedSize.value.bytes;
  stInstantMbps.value = 0;
  stAvgMbps.value = 0;
  stMaxMbps.value = 0;
  stDoneResult.value = null;
  stMessage.value = null;
  latencyMs.value = null;
  jitterMs.value = null;
  edgeColo.value = null;
}

async function startSpeedtest() {
  if (stStarting.value || stRunning.value) return;

  stStarting.value = true;
  try {
    resetSpeedtestUi();
    stRunning.value = true;

    latencyRunning.value = true;
    try {
      await invoke("measure_latency");
    } catch {
      latencyRunning.value = false;
      latencyMs.value = null;
      jitterMs.value = null;
      edgeColo.value = null;
    }

    const size = selectedSize.value?.bytes ?? sizeOptions.value[2].bytes;
    const dir = speedDirection.value ?? "download";

    const setting = {
      direction: dir,
      target_bytes: size,
      max_duration_ms: maxDurationMs,
    };

    await invoke("start_speedtest", { setting });
  } finally {
    stStarting.value = false;
  }
}

async function stopSpeedtest() {
  stStarting.value = false;
  stRunning.value = false;
  latencyRunning.value = false;
  stInstantMbps.value = 0;

  await invoke("stop_speedtest");
}

function badgeLabel() {
  if (stStarting.value) return "Preparing";

  if (latencyRunning.value) return "Measuring Latency";

  if (stRunning.value) {
    return speedDirection.value === "download"
      ? "Testing Download"
      : "Testing Upload";
  }

  if (!stDoneResult.value) return "Idle";

  switch (stDoneResult.value) {
    case "full":
      return "Done";
    case "timeout":
      return "Timeout";
    case "canceled":
      return "Canceled";
    case "error":
      return "Error";
  }
}

function badgeSeverity() {
  if (stStarting.value || latencyRunning.value) return "info";

  if (stRunning.value) return "primary";

  if (!stDoneResult.value) return "secondary";

  switch (stDoneResult.value) {
    case "full":
      return "success";
    case "timeout":
      return "warning";
    case "canceled":
      return "secondary";
    case "error":
      return "danger";
  }
}

let unlistenLatencyDone: UnlistenFn | null = null;
let unlistenUpdate: UnlistenFn | null = null;
let unlistenDone: UnlistenFn | null = null;

watch(speedDirection, () => {
  if (!stRunning.value) resetSpeedtestUi();
});
watch(selectedSize, () => {
  if (!stRunning.value) resetSpeedtestUi();
});

onMounted(async () => {
  // initial refresh: IP info
  refresh();

  // speedtest event listeners
  unlistenLatencyDone = await listen("latency:done", (e:any) => {
    const p = e.payload;
    latencyMs.value = p.latency_ms ?? null;
    jitterMs.value = p.jitter_ms ?? null;
    edgeColo.value = p.colo ?? null;
    latencyRunning.value = false;
  });

  unlistenUpdate = await listen("speedtest:update", (e: any) => {
    const p = e.payload;
    if (!p) return;
    if (p.direction !== speedDirection.value) return;

    stRunning.value = true;
    stDoneResult.value = null;
    stMessage.value = null;

    stElapsedMs.value = p.elapsed_ms ?? 0;
    stTransferred.value = p.transferred_bytes ?? 0;
    stTarget.value = p.target_bytes ?? selectedSize.value.bytes;
    stInstantMbps.value = p.instant_mbps ?? 0;
    stAvgMbps.value = p.avg_mbps ?? 0;
    stMaxMbps.value = Math.max(stMaxMbps.value, stInstantMbps.value);
  });

  unlistenDone = await listen("speedtest:done", (e: any) => {
    const p = e.payload;
    if (!p) return;
    if (p.direction !== speedDirection.value) return;

    stRunning.value = false;
    stElapsedMs.value = p.elapsed_ms ?? stElapsedMs.value;
    stTransferred.value = p.transferred_bytes ?? stTransferred.value;
    stTarget.value = p.target_bytes ?? stTarget.value;
    stAvgMbps.value = p.avg_mbps ?? stAvgMbps.value;

    stDoneResult.value = (p.result ?? "error") as Result;
    stMessage.value = p.message ?? null;
    stInstantMbps.value = 0;
  });
});

onBeforeUnmount(async () => {
  if (unlistenUpdate) await unlistenUpdate();
  if (unlistenDone) await unlistenDone();
  if (unlistenLatencyDone) await unlistenLatencyDone();
});

function fmtDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  const m = Math.floor(s / 60);
  const ss = s % 60;
  const mm = m % 60;
  if (m > 0) return `${mm}:${ss.toString().padStart(2, "0")}`;
  return `${ss}s`;
}
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2">
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm">Public IP Information</span>
      </div>
      <div class="flex items-center gap-2 justify-end">
        <Button outlined :icon="publicIpVisible ? 'pi pi-eye' : 'pi pi-eye-slash'" @click="togglePublicIp" class="icon-btn" severity="secondary" />
        <Button outlined icon="pi pi-refresh" :loading="loading" @click="refresh" class="icon-btn" severity="secondary" />
      </div>
    </div>

    <div class="flex-1 min-h-0">
      <!-- Scrollable content -->
      <ScrollPanel :style="{ width: '100%', height: panelHeight }" class="flex-1 min-h-0">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <!-- IPv4 card -->
          <Card>
            <template #title>Public IPv4</template>
            <template #content>
              <div v-if="!ipv4" class="text-surface-500">No IPv4 detected.</div>
              <div v-else class="space-y-2">
                <div class="flex items-center justify-between bg-surface-50/5 rounded-lg px-3 py-2">
                  <div>
                    <div class="text-xs text-surface-500">Address</div>
                    <div class="font-mono text-sm copyable" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(ipv4.ip_addr) }}</div>
                  </div>
                </div>

                <div class="grid grid-cols-2 gap-3 text-sm">
                  <div>
                    <div class="text-surface-500 text-xs">Hostname</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv4.host_name)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">Network</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv4.network)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">ASN</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv4.asn)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">AS Name</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv4.as_name)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">Country</div>
                    <div class="truncate">
                      <div v-if="publicIpVisible">
                        {{ pubIpGate(nv(ipv4.country_name)) }}
                        <span v-if="ipv4.country_code">({{ pubIpGate(ipv4.country_code) }})</span>
                      </div>
                      <div v-else class="text-surface-500">{{ pubIpGate(nv(ipv4.country_name)) }}</div>
                    </div>
                  </div>
                </div>
              </div>
            </template>
          </Card>

          <!-- IPv6 card -->
          <Card>
            <template #title>Public IPv6</template>
            <template #content>
              <div v-if="!ipv6" class="text-surface-500">No IPv6 detected.</div>
              <div v-else class="space-y-2">
                <div class="flex items-center justify-between bg-surface-50/5 rounded-lg px-3 py-2">
                  <div>
                    <div class="text-xs text-surface-500">Address</div>
                    <div class="font-mono text-sm copyable" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(ipv6.ip_addr) }}</div>
                  </div>
                </div>

                <div class="grid grid-cols-2 gap-3 text-sm">
                  <div>
                    <div class="text-surface-500 text-xs">Hostname</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv6.host_name)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">Network</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv6.network)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">ASN</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv6.asn)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">AS Name</div>
                    <div class="truncate" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(ipv6.as_name)) }}</div>
                  </div>
                  <div>
                    <div class="text-surface-500 text-xs">Country</div>
                    <div class="truncate">
                      <div v-if="publicIpVisible">
                        {{ pubIpGate(nv(ipv6.country_name)) }}
                        <span v-if="ipv6.country_code">({{ pubIpGate(ipv6.country_code) }})</span>
                      </div>
                      <div v-else class="text-surface-500">{{ pubIpGate(nv(ipv6.country_name)) }}</div>
                    </div>
                  </div>
                </div>
              </div>
            </template>
          </Card>

          <!-- Speed Test card (full width) -->
          <Card class="md:col-span-2">
            <template #title>
              <div class="flex items-center justify-between gap-2">
                <div class="flex items-center gap-2">
                  <span>Speed Test</span>
                  <Tag :severity="badgeSeverity()" :value="badgeLabel()" />
                </div>
                <div class="flex items-center gap-2">
                  <SelectButton
                    v-model="speedDirection"
                    :options="[
                      { label: 'Download', value: 'download' },
                      { label: 'Upload', value: 'upload' }
                    ]"
                    optionLabel="label"
                    optionValue="value"
                    :disabled="stRunning || stStarting"
                    size="small"
                  />
                  <Select
                    v-model="selectedSize"
                    :options="sizeOptions"
                    optionLabel="label"
                    placeholder="Select size"
                    :disabled="stRunning || stStarting"
                    class="w-36"
                    size="small"
                  />
                  <Button
                    icon="pi pi-play"
                    label="Start"
                    @click="startSpeedtest"
                    :disabled="stRunning || stStarting"
                    size="small"
                  />
                  <Button
                    icon="pi pi-stop"
                    label="Stop"
                    severity="secondary"
                    @click="stopSpeedtest"
                    :disabled="!stRunning && !stStarting"
                    size="small"
                  />
                </div>
              </div>
            </template>

            <template #content>
              <div class="grid grid-cols-1 md:grid-cols-4 gap-3">
                <div class="rounded-lg bg-surface-50/5 px-4 py-3">
                  <div class="text-xs text-surface-500">Current</div>
                  <div class="text-2xl font-semibold tabular-nums">{{ stInstantMbps.toFixed(1) }} Mbps</div>
                </div>

                <div class="rounded-lg bg-surface-50/5 px-4 py-3">
                  <div class="text-xs text-surface-500">Average</div>
                  <div class="text-2xl font-semibold tabular-nums">{{ stAvgMbps.toFixed(1) }} Mbps</div>
                </div>

                <div class="rounded-lg bg-surface-50/5 px-4 py-3">
                  <div class="text-xs text-surface-500">Max</div>
                  <div class="text-2xl font-semibold tabular-nums">{{ stMaxMbps.toFixed(1) }} Mbps</div>
                </div>

                <div class="rounded-lg bg-surface-50/5 px-4 py-3">
                  <div class="flex items-center justify-between">
                    <div class="text-xs text-surface-500">Latency</div>
                    <span v-if="latencyRunning" class="text-surface-500">Measuring...</span>
                    <span v-else-if="edgeColo" class="text-xs text-surface-500 font-mono">{{ edgeColo }}</span>
                  </div>
                  <div class="text-2xl font-semibold tabular-nums">
                    <span v-if="latencyRunning" class="text-surface-500">...</span>
                    <span v-else-if="latencyMs !== null">{{ latencyMs.toFixed(0) }} ms</span>
                    <span v-else class="text-surface-500">-</span>
                  </div>
                  <div class="mt-1 text-xs text-surface-500">
                    Jitter:
                    <span v-if="jitterMs !== null" class="text-surface-900 dark:text-surface-0 tabular-nums">{{ jitterMs.toFixed(0) }} ms</span>
                    <span v-else>-</span>
                    <span class="ml-2 tabular-nums">Elapsed: {{ elapsedText }}</span>
                  </div>
                </div>
              </div>

              <div class="mt-3">
                <div class="flex items-center justify-between text-sm">
                  <div class="text-surface-500">
                    Progress: <span class="text-surface-900 dark:text-surface-0">{{ transferredText }}</span> / <span class="text-surface-900 dark:text-surface-0">{{ targetText }}</span>
                  </div>
                  <div class="text-surface-500 tabular-nums">
                    {{ progressPct.toFixed(1) }}%
                  </div>
                </div>

                <ProgressBar :value="progressPct" class="mt-2" />

                <div v-if="stDoneResult === 'timeout'" class="mt-2 text-sm text-yellow-500">
                  Timed out (30s). Speed is calculated from actual transferred bytes / actual elapsed time.
                </div>
                <div v-if="stDoneResult === 'full'" class="mt-2 text-sm text-green-500">
                  Completed full transfer.
                </div>
                <div v-if="stDoneResult === 'canceled'" class="mt-2 text-sm text-surface-500">
                  Canceled.
                </div>
                <div v-if="stDoneResult === 'error'" class="mt-2 text-sm text-red-500">
                  Error: {{ stMessage ?? "Unknown error" }}
                </div>
              </div>
            </template>
          </Card>
        </div>
      </ScrollPanel>
    </div>
  </div>
</template>
