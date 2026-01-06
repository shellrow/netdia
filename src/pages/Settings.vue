<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { AppConfig } from "../types/config";
import { useTheme } from "../composables/useTheme";
import { normalizeBpsUnit, readBpsUnit, LOCAL_SETTINGS } from "../utils/preferences";

const { themeMode, setSystemTheme, setLightTheme, setDarkTheme } = useTheme();

type SectionKey = "general" | "appearance" | "app";
type Section = { key: SectionKey; label: string; icon: string; desc?: string };

const SECTIONS: Section[] = [
  { key: "general",    label: "General",    icon: "pi-sliders-h", desc: "Manage startup, behavior, and refresh settings." },
  { key: "appearance", label: "Appearance", icon: "pi-desktop",   desc: "Customize color themes and display units." },
  { key: "app",        label: "App",        icon: "pi-box",       desc: "Update and log settings." },
];

const current = ref<SectionKey>("general");
const currentSection = computed(() => SECTIONS.find(s => s.key === current.value)!);

const baseItem = "flex items-center cursor-pointer p-3 gap-2 rounded-lg border border-transparent transition-colors duration-150";
const idleColor = "text-surface-700 dark:text-surface-200 hover:bg-surface-50 dark:hover:bg-surface-800 hover:border-surface-100 dark:hover:border-surface-700 hover:text-surface-900 dark:hover:text-surface-50";
const activeColor = "bg-surface-50 dark:bg-surface-800 text-surface-900 dark:text-surface-50 border-surface-200 dark:border-surface-700";
const itemClass = (active: boolean) => `${baseItem} ${active ? activeColor : idleColor}`;

// Tauri app config
const cfg = ref<AppConfig | null>(null);
const loading = ref(false);
const saving  = ref(false);

const autostart   = ref(localStorage.getItem(LOCAL_SETTINGS.autostart) === "1");
const theme = computed<"system" | "light" | "dark">({
  get: () => themeMode.value,
  set: (v) => {
    if (v === "system") setSystemTheme();
    else if (v === "light") setLightTheme();
    else setDarkTheme();
  },
});
const refreshMs   = ref<number>(parseInt(localStorage.getItem(LOCAL_SETTINGS.refresh) || "1000", 10));
const bpsUnit     = ref<"bytes"|"bits">(readBpsUnit(localStorage));

type LogsPath = { folder: string; file?: string | null };

const opening = ref(false);

watch(autostart,   v => localStorage.setItem(LOCAL_SETTINGS.autostart, v ? "1" : "0"));
watch(theme,       v => localStorage.setItem(LOCAL_SETTINGS.theme,     v));
watch(refreshMs,   v => localStorage.setItem(LOCAL_SETTINGS.refresh,   String(v)));
watch(bpsUnit,     v => localStorage.setItem(LOCAL_SETTINGS.bpsUnit,   v));

const fmtMs = (v:number) => `${v} ms`;

function applyFromConfig(c: AppConfig) {
  autostart.value = c.startup;
  theme.value     = c.theme;
  refreshMs.value = c.refresh_interval_ms;
  bpsUnit.value   = normalizeBpsUnit(c.data_unit);
}

async function loadConfig() {
  loading.value = true;
  try {
    const c = await invoke<AppConfig>("get_config");
    cfg.value = c;
    applyFromConfig(c);
  } finally {
    loading.value = false;
  }
}

let saveTimer: number | null = null;
function scheduleSave() {
  if (saveTimer) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(saveConfig, 350); // debounce 350ms
}

async function saveConfig() {
  if (!cfg.value) return;
  saving.value = true;
  try {
    const next: AppConfig = {
      ...cfg.value,
      startup: autostart.value,
      theme: theme.value,
      refresh_interval_ms: refreshMs.value,
      data_unit: bpsUnit.value,
      logging: cfg.value.logging,
    };
    await invoke("save_config", { cfg: next });
    cfg.value = next;
  } finally {
    saving.value = false;
  }
}

async function openLogsFolder() {
  try {
    opening.value = true;
    const paths = await invoke<LogsPath>("logs_dir_path");
    // Try to reveal the log file in the folder
    if (paths.file) {
      try {
        await revealItemInDir(paths.file);
        return;
      } catch (err) {
        console.warn("revealItemInDir failed, fallback to openPath", err);
      }
    }
    // Fallback: just open the folder
    await openPath(paths.folder);
  } catch (e: any) {
    alert(`Failed to open logs folder:\n${e?.toString?.() ?? e}`);
  } finally {
    opening.value = false;
  }
}

// Watchers to auto-save on change
watch([autostart, theme, refreshMs, bpsUnit], scheduleSave, { deep: false });

onMounted(loadConfig);

</script>

<template>
  <div class="p-4 h-full min-h-0 flex flex-col gap-4">
    <div class="flex-1 min-h-0 grid grid-cols-1 md:grid-cols-[150px_1fr] gap-4">
      <!-- Sidebar -->
      <aside class="rounded-2xl border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 p-2 overflow-auto">
        <ul class="list-none m-0 p-0 flex flex-col gap-1">
          <li v-for="s in SECTIONS" :key="s.key">
            <button
              type="button"
              :class="itemClass(current === s.key)"
              @click="current = s.key"
            >
              <i :class="['pi', s.icon, 'text-surface-500 dark:text-surface-400']" />
              <span class="font-medium text-sm leading-snug">{{ s.label }}</span>
            </button>
          </li>
        </ul>
      </aside>
      <!-- Content panel -->
      <section class="rounded-2xl border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-950 p-4 min-h-0 overflow-auto">
        <header class="mb-4">
          <div class="text-lg font-semibold">{{ currentSection.label }}</div>
          <div class="text-sm text-surface-500 mt-1">{{ currentSection.desc }}</div>
        </header>
        <!-- General -->
        <div v-if="current === 'general'" class="flex flex-col gap-4">
          <Card>
            <template #title>Startup & Behavior</template>
            <template #content>
              <div class="flex items-center justify-between py-2">
                <div>
                  <div class="font-medium">Launch NetDia on system startup</div>
                  <div class="text-sm text-surface-500">Automatically start the app after login (coming soon).</div>
                </div>
                <ToggleSwitch v-model="autostart" disabled />
              </div>
            </template>
          </Card>

          <Card>
            <template #title>Refresh interval</template>
            <template #content>
              <div class="grid grid-cols-1 sm:grid-cols-[1fr_auto] items-center gap-3">
                <div>
                  <div class="font-medium">Dashboard & interface stats update</div>
                  <div class="text-sm text-surface-500">Adjust to balance performance and responsiveness.</div>
                </div>
                <div class="flex items-center gap-2">
                  <InputNumber v-model="refreshMs" :min="1000" :max="10000" :step="100" showButtons inputClass="w-28" />
                  <span class="text-sm text-surface-500">{{ fmtMs(refreshMs) }}</span>
                </div>
              </div>
            </template>
          </Card>
        </div>

        <!-- Appearance -->
        <div v-else-if="current === 'appearance'" class="flex flex-col gap-4">
          <Card>
            <template #title>Theme</template>
            <template #content>
              <div class="flex flex-col gap-2">
                <div class="flex flex-wrap items-center gap-3">
                  <RadioButton v-model="theme" inputId="th-system" value="system" />
                  <label for="th-system">System</label>
                  <RadioButton v-model="theme" inputId="th-light" value="light" />
                  <label for="th-light">Light</label>
                  <RadioButton v-model="theme" inputId="th-dark" value="dark" />
                  <label for="th-dark">Dark</label>
                </div>
                <div class="text-sm text-surface-500">Affects the overall appearance of the app.</div>
              </div>
            </template>
          </Card>

          <Card>
            <template #title>Display units</template>
            <template #content>
              <div class="flex flex-col gap-2">
                <div class="font-medium">Throughput unit</div>
                <div class="flex flex-wrap items-center gap-3">
                  <RadioButton v-model="bpsUnit" inputId="u-bits" value="bits" />
                  <label for="u-bits">bps (bits)</label>
                  <RadioButton v-model="bpsUnit" inputId="u-bytes" value="bytes" />
                  <label for="u-bytes">B/s (bytes)</label>
                </div>
                <div class="text-sm text-surface-500">Affects RX/TX display values throughout the UI.</div>
              </div>
            </template>
          </Card>
        </div>

        <!-- App -->
        <div v-else-if="current === 'app'" class="flex flex-col gap-4">
          <Card>
            <template #title>Updates</template>
            <template #content>
              <div class="flex items-center justify-between py-1">
                <div>
                  <div class="font-medium">Check for updates</div>
                  <div class="text-sm text-surface-500">Manual update check (coming soon).</div>
                </div>
                <Button label="Check now" icon="pi pi-refresh" outlined disabled />
              </div>
            </template>
          </Card>

          <Card>
            <template #title>Logs</template>
            <template #content>
              <div class="text-sm text-surface-500 mb-3">
                Configure log level and output path (coming soon).
              </div>
              <div class="flex gap-2">
                <Button label="Open logs folder" icon="pi pi-folder-open" outlined @click="openLogsFolder" />
              </div>
            </template>
          </Card>
        </div>
      </section>
    </div>
  </div>
</template>
