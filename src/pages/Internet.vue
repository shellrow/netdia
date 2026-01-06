<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { IpInfo, IpInfoDual } from "../types/internet";
import { nv } from "../utils/formatter";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { usePrivacyGate } from "../composables/usePrivacyGate";

// Reactive states
const loading = ref(false);
const ipv4 = ref<IpInfo | null>(null);
const ipv6 = ref<IpInfo | null>(null);
const { publicIpVisible, togglePublicIp, pubIpGate } = usePrivacyGate();

// Fetch both IPv4 and IPv6 info
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

onMounted(refresh);

// @ts-ignore -- used in template refs
const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2">
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm">Public IP Information</span>
      </div>
      <div class="flex items-center gap-2 justify-end">
        <Button outlined :icon="publicIpVisible ? 'pi pi-eye' : 'pi pi-eye-slash'" @click="togglePublicIp" class="w-9 h-9" severity="secondary" />
        <Button outlined icon="pi pi-refresh" :loading="loading" @click="refresh" class="w-9 h-9" severity="secondary" />
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
      </div>
    </ScrollPanel>
    </div>
  </div>
</template>
