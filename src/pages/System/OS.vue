<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { SysInfo } from "../../types/system";
import { useScrollPanelHeight } from "../../composables/useScrollPanelHeight";
import { usePrivacyGate } from "../../composables/usePrivacyGate";

const loading = ref(false);
const sys = ref<SysInfo | null>(null);
async function fetchSys() { loading.value = true; try { sys.value = await invoke("get_sys_info") as SysInfo; } finally { loading.value = false; } }
onMounted(fetchSys);

function nv(v?: string | null) { return v && v.trim() ? v : "-"; }

const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();
const { hostnameVisible, toggleHostname, hostnameGate } = usePrivacyGate();
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-0 lg:px-4 lg:pt-4 lg:pb-0 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2">
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm" v-if="sys">
          OS info and network related settings ({{ sys.os_type }})
        </span>
      </div>
      <div class="flex items-center gap-2 justify-end">
        <Button outlined :icon="hostnameVisible ? 'pi pi-eye' : 'pi pi-eye-slash'" @click="toggleHostname" class="w-9 h-9" severity="secondary" />
        <Button outlined icon="pi pi-refresh" :loading="loading" @click="fetchSys" class="w-9 h-9" severity="secondary" />
      </div>
    </div>

    <div class="flex-1 min-h-0">
    <!-- ScrollPanel -->
    <ScrollPanel :style="{ width: '100%', height: panelHeight }" class="flex-1 min-h-0">
      <div class="grid grid-cols-1 xl:grid-cols-3 gap-3">
        <!-- System Overview -->
        <Card>
          <template #title>System Overview</template>
          <template #content>
            <div v-if="!sys" class="text-surface-500">Loading...</div>
            <div v-else class="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-2 text-sm">
              <div class="text-surface-500">Hostname</div>
              <div class="font-medium truncate" :class="{ 'text-surface-500': !hostnameVisible }">{{ hostnameGate(sys.hostname) }}</div>
              <div class="text-surface-500">OS</div>
              <div class="font-medium">{{ sys.os_type }} {{ sys.os_version }}</div>
              <div class="text-surface-500">Kernel</div>
              <div class="font-medium">{{ nv(sys.kernel_version) }}</div>
              <div class="text-surface-500">Edition</div>
              <div class="font-medium">{{ sys.edition }}</div>
              <div class="text-surface-500">Codename</div>
              <div class="font-medium">{{ sys.codename }}</div>
              <div class="text-surface-500">Architecture</div>
              <div class="font-medium">{{ sys.architecture }} ({{ sys.bitness }})</div>
            </div>
          </template>
        </Card>

        <!-- Proxy Environment -->
        <Card>
          <template #title>Proxy Environment</template>
          <template #content>
            <div v-if="!sys" class="text-surface-500">Loading...</div>
            <div v-else class="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-2 text-sm">
              <div class="text-surface-500">HTTP_PROXY</div>
              <div class="font-mono break-all">{{ nv(sys.proxy.http) }}</div>
              <div class="text-surface-500">HTTPS_PROXY</div>
              <div class="font-mono break-all">{{ nv(sys.proxy.https) }}</div>
              <div class="text-surface-500">ALL_PROXY</div>
              <div class="font-mono break-all">{{ nv(sys.proxy.all) }}</div>
              <div class="text-surface-500">NO_PROXY</div>
              <div class="font-mono break-all">{{ nv(sys.proxy.no_proxy) }}</div>
            </div>
          </template>
        </Card>
      </div>
    </ScrollPanel>
    </div>
  </div>
</template>