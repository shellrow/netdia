<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import DataTable from 'primevue/datatable';
import Column from 'primevue/column';
import { DomainLookupInfo } from "../types/dns";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { STORAGE_KEYS } from "../constants/storage";

const q = ref(localStorage.getItem(STORAGE_KEYS.LAST_DNS_QUERY) || "example.com");
const loading = ref(false);
const err = ref<string | null>(null);
const data = ref<DomainLookupInfo | null>(null);

async function runLookup() {
  const target = q.value.trim();
  if (!target) return;
  loading.value = true;
  err.value = null;
  data.value = null;

  try {
    const res = await invoke<DomainLookupInfo>("lookup_all", { hostname: target });
    data.value = res;
    localStorage.setItem(STORAGE_KEYS.LAST_DNS_QUERY, target);
  } catch (e: any) {
    err.value = String(e?.message ?? e ?? "Lookup failed");
  } finally {
    loading.value = false;
  }
}

function onEnter(e: KeyboardEvent) {
  if (e.key === "Enter") runLookup();
}

const hasAny = (obj: DomainLookupInfo | null) =>
  !!obj &&
  ((obj.a?.length ?? 0) > 0 ||
    (obj.aaaa?.length ?? 0) > 0 ||
    (obj.mx?.length ?? 0) > 0 ||
    (obj.ns?.length ?? 0) > 0 ||
    (obj.soa?.length ?? 0) > 0 ||
    (obj.srv?.length ?? 0) > 0 ||
    (obj.tlsa?.length ?? 0) > 0 ||
    (obj.txt?.length ?? 0) > 0 ||
    (obj.cert?.length ?? 0) > 0);

// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2">
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm">DNS Records</span>
      </div>
      <div class="flex items-center gap-2 justify-end">
        <InputGroup class="max-w-[420px] w-full">
          <InputGroupAddon><i class="pi pi-globe" /></InputGroupAddon>
          <InputText
            v-model="q"
            placeholder="domain (e.g. example.com)"
            @keydown="onEnter"
            class="flex-1 min-w-0"
          />
          <Button label="Lookup" icon="pi pi-search" :loading="loading" @click="runLookup" />
        </InputGroup>
      </div>
    </div>

    <div class="flex-1 min-h-0">
    <!-- Scrollable content -->
    <ScrollPanel :style="{ width: '100%', height: panelHeight }" class="flex-1 min-h-0">
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-2 content-start auto-rows-max p-3">
        <!-- Summary -->
        <Card>
          <template #title>Summary</template>
          <template #content>
            <div v-if="err" class="text-red-500 text-sm">{{ err }}</div>
            <div v-else-if="loading" class="text-surface-500">Resolving...</div>
            <div v-else-if="data">
              <div class="flex flex-wrap items-center gap-2 mb-2">
                <span class="text-sm text-surface-500">Domain</span>
                <span class="font-medium">{{ data.name }}</span>
              </div>

              <div class="mb-3">
                <div class="text-surface-500 text-xs">A Records</div>
                <div class="mt-1 flex flex-wrap gap-2">
                  <Chip v-for="(ip, i) in data.a" :key="'a-'+i" :label="ip" class="font-mono copyable" />
                  <span v-if="(data.a?.length ?? 0) === 0" class="text-surface-500 text-sm">-</span>
                </div>
              </div>

              <div>
                <div class="text-surface-500 text-xs">AAAA Records</div>
                <div class="mt-1 flex flex-wrap gap-2">
                  <Chip v-for="(ip, i) in data.aaaa" :key="'aaaa-'+i" :label="ip" class="font-mono copyable" />
                  <span v-if="(data.aaaa?.length ?? 0) === 0" class="text-surface-500 text-sm">-</span>
                </div>
              </div>
            </div>

            <div v-else class="text-surface-500">
              Enter a domain and click <b>Lookup</b>.
            </div>
          </template>
        </Card>

        <!-- NS -->
          <Card>
          <template #title>Nameservers</template>
          <template #content>
              <div v-if="data">
              <div class="flex flex-wrap gap-2">
                  <Chip v-for="(ns, i) in data.ns" :key="'ns-'+i" :label="ns" class="font-mono copyable" />
                  <span v-if="(data.ns?.length ?? 0) === 0" class="text-surface-500 text-sm">-</span>
              </div>
              </div>
              <div v-else class="text-surface-500 text-sm">-</div>
          </template>
          </Card>

          <!-- MX -->
          <Card>
          <template #title>MX Records</template>
          <template #content>
              <DataTable v-if="data && (data.mx?.length ?? 0) > 0" :value="data.mx" size="small" class="text-sm copyable">
              <Column field="preference" header="Pref" style="width: 90px" />
              <Column field="exchange" header="Exchange" />
              </DataTable>
              <div v-else class="text-surface-500 text-sm">-</div>
          </template>
          </Card>

          <!-- SOA -->
          <Card>
          <template #title>SOA</template>
          <template #content>
              <DataTable v-if="data && (data.soa?.length ?? 0) > 0" :value="data.soa" size="small" class="text-sm copyable">
                <Column field="mname" header="MNAME" />
                <Column field="rname" header="RNAME" />
                <Column field="serial" header="Serial" style="width: 120px" />
                <Column field="refresh" header="Refresh" style="width: 110px" />
                <Column field="retry" header="Retry" style="width: 100px" />
                <Column field="expire" header="Expire" style="width: 110px" />
                <Column field="minimum" header="Minimum" style="width: 120px" />
              </DataTable>
              <div v-else class="text-surface-500 text-sm">-</div>
          </template>
          </Card>

          <!-- SRV -->
          <Card>
          <template #title>SRV</template>
          <template #content>
              <DataTable v-if="data && (data.srv?.length ?? 0) > 0" :value="data.srv" size="small" class="text-sm copyable">
                <Column field="priority" header="Priority" style="width: 100px" />
                <Column field="weight" header="Weight" style="width: 100px" />
                <Column field="port" header="Port" style="width: 100px" />
                <Column field="target" header="Target" />
              </DataTable>
              <div v-else class="text-surface-500 text-sm">-</div>
          </template>
          </Card>

          <!-- TLSA -->
          <Card>
          <template #title>TLSA</template>
          <template #content>
              <DataTable v-if="data && (data.tlsa?.length ?? 0) > 0" :value="data.tlsa" size="small" class="text-sm copyable">
                <Column field="cert_usage" header="Usage" style="width: 90px" />
                <Column field="selector" header="Selector" style="width: 100px" />
                <Column field="matching" header="Matching" style="width: 100px" />
                <Column header="Cert (base64)">
                    <template #body="{ data: row }">
                    <span class="font-mono break-all">{{ row.cert_data_base64 }}</span>
                    </template>
                </Column>
              </DataTable>
              <div v-else class="text-surface-500 text-sm">-</div>
          </template>
          </Card>

          <!-- TXT -->
          <Card>
          <template #title>TXT</template>
          <template #content>
              <DataTable v-if="data && (data.txt?.length ?? 0) > 0" :value="data.txt" size="small" class="text-sm copyable">
                <Column field="key" header="Key" style="width: 160px" />
                <Column header="Value">
                    <template #body="{ data: row }">
                    <span class="font-mono break-all">{{ row.value }}</span>
                    </template>
                </Column>
              </DataTable>
              <div v-else class="text-surface-500 text-sm">-</div>
          </template>
          </Card>

          <!-- CERT -->
          <Card>
          <template #title>CERT</template>
          <template #content>
              <DataTable v-if="data && (data.cert?.length ?? 0) > 0" :value="data.cert" size="small" class="text-sm copyable">
                <Column field="cert_type" header="Type" style="width: 90px" />
                <Column field="key_tag" header="KeyTag" style="width: 100px" />
                <Column field="algorithm" header="Alg" style="width: 80px" />
                <Column header="Data (base64)">
                    <template #body="{ data: row }">
                    <span class="font-mono break-all">{{ row.cert_data_base64 }}</span>
                    </template>
                </Column>
              </DataTable>
              <div v-else class="text-surface-500 text-sm">-</div>
          </template>
          </Card>
      </div>
      <div v-if="data && !hasAny(data)" class="text-surface-500 text-sm mt-4">
          No DNS records found for this name.
      </div>
    </ScrollPanel>
    </div>
  </div>
</template>