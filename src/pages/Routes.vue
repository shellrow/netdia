<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import DataTable from 'primevue/datatable';
import Column from 'primevue/column';
import type { RouteEntry, RouteFamily } from "../types/route";
import { flagShort, flagDescription } from "../types/route";
import { cidr } from "../types/net";

const wrapRef = ref<HTMLElement|null>(null);
const toolbarRef = ref<HTMLElement|null>(null);
const tableHeight = ref("400px");  

let ro: ResizeObserver | null = null;
let rafId: number | null = null;
let scheduled = false;

const loading = ref(false);
const routes = ref<RouteEntry[]>([]);
const q = ref("");
const family = ref<RouteFamily>("All");

function _calcTableHeight(): string {
  const wrap = wrapRef.value;
  if (!wrap) return tableHeight.value;

  const cs = getComputedStyle(wrap);
  const padY = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
  const inner = wrap.clientHeight - padY;

  const toolbarH = toolbarRef.value?.offsetHeight ?? 0;
  const EXTRA_OFFSET = 80;
  const GAP = 12;
  const px = Math.max(200, inner - toolbarH - EXTRA_OFFSET - GAP);
  return `${Math.floor(px)}px`;
}

function scheduleRecalc() {
  if (scheduled) return;
  scheduled = true;
  rafId && cancelAnimationFrame(rafId);
  rafId = requestAnimationFrame(() => {
    scheduled = false;
    const next = _calcTableHeight();
    if (next !== tableHeight.value) {
      tableHeight.value = next;
    }
  });
}

async function fetchRoutes() {
  loading.value = true;
  try {
    routes.value = (await invoke("get_routes")) as RouteEntry[];
  } finally {
    loading.value = false;
  }
}

const filtered = computed(() => {
  let xs = routes.value;
  if (family.value !== "All") {
    xs = xs.filter(r => r.family === family.value);
  }
  const s = q.value.trim().toLowerCase();
  if (!s) return xs;
  return xs.filter(r => {
    const hay = [
      cidr(r.destination.addr, r.destination.prefix_len),
      r.gateway ?? "",
      r.ifname ?? "",
      (r.flags ?? []).join(" "),
      r.protocol ?? "",
      r.scope ?? "",
    ].join(" ").toLowerCase();
    return hay.includes(s);
  });
});

onMounted(async () => {
  await fetchRoutes();
  await nextTick();
  tableHeight.value = _calcTableHeight();
  ro = new ResizeObserver(() => {
    scheduleRecalc();
  });
  if (wrapRef.value) ro.observe(wrapRef.value);
  if (toolbarRef.value) ro.observe(toolbarRef.value);
  window.addEventListener("resize", scheduleRecalc);
});

onBeforeUnmount(() => {
  ro?.disconnect();
  if (rafId) cancelAnimationFrame(rafId);
  window.removeEventListener("resize", scheduleRecalc);
});
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2">
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm">Routing Table ({{ filtered.length }})</span>
      </div>
      <div class="flex items-center gap-2 justify-end">
        <Select v-model="family" :options="[{label:'All', value:'All'},{label:'IPv4', value:'Ipv4'},{label:'IPv6', value:'Ipv6'}]" optionLabel="label" optionValue="value" class="w-28" size="small" />
        <InputGroup class="max-w-[220px]">
          <InputGroupAddon><i class="pi pi-search"/></InputGroupAddon>
          <InputText v-model="q" placeholder="Search (dst/gw/if/flags...)" size="small" />
        </InputGroup>
        <Button outlined icon="pi pi-refresh" :loading="loading" @click="fetchRoutes" class="icon-btn" severity="secondary" />
      </div>
    </div>

    <!-- Table -->
    <DataTable
      :value="filtered"
      size="small"
      :loading="loading"
      paginator
      :rows="20"
      :rowsPerPageOptions="[20,50,100]"
      sortMode="single"
      scrollable
      :scrollHeight="tableHeight"
      class="text-sm copyable"
      stripedRows 
      resizableColumns 
      columnResizeMode="fit"
    >
      <Column field="destination" header="Destination" sortable style="min-width: 220px">
        <template #body="{ data }">
          <span class="font-medium">{{ cidr(data.destination.addr, data.destination.prefix_len) }}</span>
          <Tag v-if="cidr(data.destination.addr, data.destination.prefix_len)==='0.0.0.0/0' || cidr(data.destination.addr, data.destination.prefix_len)==='::/0'" value="Default" severity="secondary" class="ml-2" />
        </template>
      </Column>

      <Column header="Gateway" style="min-width: 200px">
        <template #body="{ data }">
          <span v-if="data.on_link" class="text-surface-500">on-link</span>
          <span v-else>{{ data.gateway || '-' }}</span>
        </template>
      </Column>

      <Column field="ifname" header="Iface" sortable style="width: 120px" />
      <Column field="metric" header="Metric" sortable style="width: 90px">
        <template #body="{ data }">{{ data.metric ?? '-' }}</template>
      </Column>
      
      <Column header="Flags" style="min-width: 120px">
        <template #body="{ data }">
          <div class="flex gap-1 flex-wrap">
            <Tag
              v-for="(f, _i) in (data.flags || [])"
              :key="typeof f === 'string' ? f : `Other:${f.Other}`"
              :value="flagShort(f)"
              severity="secondary"
              v-tooltip.top="flagDescription(f)"
              class="cursor-help"
            />
          </div>
        </template>
      </Column>

      <Column field="protocol" header="Proto" sortable style="width: 120px">
        <template #body="{ data }">{{ data.protocol ?? '-' }}</template>
      </Column>

      <Column field="scope" header="Scope" sortable style="width: 110px">
        <template #body="{ data }">{{ data.scope ?? '-' }}</template>
      </Column>

      <Column field="lifetime_ms" header="Expire" sortable style="width: 110px">
        <template #body="{ data }">
          <span v-if="data.lifetime_ms != null">{{ Math.round((data.lifetime_ms as number)/1000) }}s</span>
          <span v-else>-</span>
        </template>
      </Column>
    </DataTable>
  </div>
</template>
