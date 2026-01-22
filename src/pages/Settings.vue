<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getVersion as getAppVersion } from "@tauri-apps/api/app";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { AppConfig } from "../types/config";
import { useTheme } from "../composables/useTheme";
import { normalizeBpsUnit, readBpsUnit } from "../utils/preferences";
import { INTERNET_CHECK_INTERVAL } from "../constants/defaults";
import { STORAGE_KEYS } from "../constants/storage";
import { clampInt } from "../utils/numeric";
import { useUpdater } from "../composables/useUpdater";

const { themeMode, setSystemTheme, setLightTheme, setDarkTheme } = useTheme();
const updater = useUpdater();

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
const theme = computed<"system" | "light" | "dark">({
  get: () => themeMode.value,
  set: (v) => {
    if (v === "system") setSystemTheme();
    else if (v === "light") setLightTheme();
    else setDarkTheme();
  },
});
const refreshMs   = ref<number>(parseInt(localStorage.getItem(STORAGE_KEYS.REFRESH_INTERVAL_MS) || "1000", 10));
const bpsUnit     = ref<"bytes"|"bits">(readBpsUnit(localStorage));

// --- Internet check settings ---
function readAutoInternetCheck(): boolean {
  const v = localStorage.getItem(STORAGE_KEYS.AUTO_INTERNET_CHECK);
  if (v == null) return true;
  return v === "1" || v.toLowerCase() === "true";
}

function readAutoInternetCheckIntervalS(): number {
  const raw = localStorage.getItem(STORAGE_KEYS.AUTO_INTERNET_CHECK_INTERVAL_S);
  if (raw == null || raw.trim() === "") return INTERNET_CHECK_INTERVAL.DEFAULT;
  const n = Number(raw);
  if (!Number.isFinite(n)) return INTERNET_CHECK_INTERVAL.DEFAULT;
  return clampInt(Math.floor(n), INTERNET_CHECK_INTERVAL.MIN, INTERNET_CHECK_INTERVAL.MAX);
}

const autoInternetCheck = ref<boolean>(readAutoInternetCheck());
const autoInternetCheckIntervalS = ref<number>(readAutoInternetCheckIntervalS());

watch(theme,     v => localStorage.setItem(STORAGE_KEYS.THEME,     v));
watch(refreshMs, v => localStorage.setItem(STORAGE_KEYS.REFRESH_INTERVAL_MS,   String(v)));
watch(bpsUnit,   v => localStorage.setItem(STORAGE_KEYS.BPS_UNIT,   v));

watch(autoInternetCheck, v => localStorage.setItem(STORAGE_KEYS.AUTO_INTERNET_CHECK, v ? "1" : "0"));
watch(autoInternetCheckIntervalS, (v) => {
  const n = Number(v);
  const next = Number.isFinite(n)
    ? clampInt(Math.floor(n), INTERNET_CHECK_INTERVAL.MIN, INTERNET_CHECK_INTERVAL.MAX)
    : INTERNET_CHECK_INTERVAL.DEFAULT;

  if (next !== autoInternetCheckIntervalS.value) {
    autoInternetCheckIntervalS.value = next;
    return;
  }

  localStorage.setItem(
    STORAGE_KEYS.AUTO_INTERNET_CHECK_INTERVAL_S,
    String(next),
  );
});

function applyFromConfig(c: AppConfig) {
  theme.value     = c.theme;
  refreshMs.value = c.refresh_interval_ms;
  bpsUnit.value   = normalizeBpsUnit(c.data_unit);

  autoInternetCheck.value = !!c.auto_internet_check;
  autoInternetCheckIntervalS.value = clampInt(Math.floor(c.auto_internet_check_interval_s ?? INTERNET_CHECK_INTERVAL.DEFAULT), INTERNET_CHECK_INTERVAL.MIN, INTERNET_CHECK_INTERVAL.MAX);
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
  saveTimer = window.setTimeout(saveConfig, 350);
}

async function saveConfig() {
  if (!cfg.value) return;
  saving.value = true;
  try {
    const next: AppConfig = {
      ...cfg.value,
      startup: false,
      theme: theme.value,
      refresh_interval_ms: refreshMs.value,
      data_unit: bpsUnit.value,
      logging: cfg.value.logging,
      auto_internet_check: autoInternetCheck.value,
      auto_internet_check_interval_s: clampInt(Math.floor(autoInternetCheckIntervalS.value), INTERNET_CHECK_INTERVAL.MIN, INTERNET_CHECK_INTERVAL.MAX),
    };
    await invoke("save_config", { cfg: next });
    cfg.value = next;
  } finally {
    saving.value = false;
  }
}

type LogsPath = { folder: string; file?: string | null };
const opening = ref(false);

async function openLogsFolder() {
  try {
    opening.value = true;
    const paths = await invoke<LogsPath>("logs_dir_path");
    if (paths.file) {
      try {
        await revealItemInDir(paths.file);
        return;
      } catch (err) {
        console.warn("revealItemInDir failed, fallback to openPath", err);
      }
    }
    await openPath(paths.folder);
  } catch (e: any) {
    alert(`Failed to open logs folder:\n${e?.toString?.() ?? e}`);
  } finally {
    opening.value = false;
  }
}

watch([theme, refreshMs, bpsUnit, autoInternetCheck, autoInternetCheckIntervalS], scheduleSave, { deep: false });

// Updater
const appVersion = ref<string>("");
const currentVersionText = computed(() => updater.info.value?.current_version ?? appVersion.value);

const pubDateText = computed(() => {
  const s = updater.info.value?.pub_date;
  if (!s) return null;
  return s.split("T")[0] ?? s;
});

function openStore() {
  const url = updater.info.value?.store_url;
  if (url) {
    openPath(url);
  }
}

onMounted(async () => {
  try {
    loadConfig();
    appVersion.value = await getAppVersion();
  } catch {
    // ignore
  }
});
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
            <template #title>Refresh interval</template>
            <template #content>
              <div class="grid grid-cols-1 sm:grid-cols-[1fr_auto] items-center gap-3">
                <div>
                  <div class="font-medium">Dashboard & interface stats update</div>
                  <div class="text-sm text-surface-500">Adjust to balance performance and responsiveness.</div>
                </div>
                <div class="flex items-center gap-2">
                  <InputNumber v-model="refreshMs" :min="1000" :max="10000" :step="100" showButtons inputClass="w-28" />
                  <span class="text-sm text-surface-500">ms</span>
                </div>
              </div>
            </template>
          </Card>
          <Card>
            <template #title>Internet connectivity check</template>
            <template #content>
              <div class="flex flex-col gap-4">
                <div class="flex items-center justify-between py-1">
                  <div>
                    <div class="font-medium">Auto Internet Check</div>
                    <div class="text-sm text-surface-500">
                      Periodically fetch public IP info to estimate reachability.
                    </div>
                  </div>
                  <ToggleSwitch v-model="autoInternetCheck" />
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-[1fr_auto] items-center gap-3">
                  <div>
                    <div class="font-medium">Interval</div>
                    <div class="text-sm text-surface-500">
                      Range: {{ INTERNET_CHECK_INTERVAL.MIN }} - {{ INTERNET_CHECK_INTERVAL.MAX }} seconds.
                    </div>
                  </div>
                  <div class="flex items-center gap-2">
                    <InputNumber
                      v-model="autoInternetCheckIntervalS"
                      :min="INTERNET_CHECK_INTERVAL.MIN"
                      :max="INTERNET_CHECK_INTERVAL.MAX"
                      :step="10"
                      showButtons
                      inputClass="w-28"
                      :disabled="!autoInternetCheck"
                    />
                    <span class="text-sm text-surface-500">s</span>
                  </div>
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
              <div class="flex flex-col gap-3">
                <div class="flex items-center justify-between">
                  <div>
                    <div class="font-medium">Application update</div>
                    <div class="text-sm text-surface-500">
                      Current version: {{ currentVersionText }}
                    </div>
                  </div>

                  <Button
                    label="Check now"
                    icon="pi pi-refresh"
                    outlined
                    :loading="updater.isChecking.value"
                    :disabled="updater.isDownloading.value"
                    @click="updater.check"
                  />
                </div>

                <!-- Status -->
                <Tag v-if="updater.state.value === 'checking'" severity="secondary" value="Checking for updates..." />
                <Tag v-else-if="updater.state.value === 'store'" severity="info" value="Updates are managed via Microsoft Store" />
                <Tag v-else-if="updater.state.value === 'available'" severity="info" value="Update available" />
                <Tag v-else-if="updater.state.value === 'downloading'" severity="info" value="Downloading update..." />
                <Tag v-else-if="updater.state.value === 'ready'" severity="warning" value="Restart required" />
                <Tag v-else-if="updater.state.value === 'idle'" severity="success" value="Up to date" />
                <Tag v-else-if="updater.state.value === 'error'" severity="danger" value="Update error" />
                <div
                  v-if="updater.state.value === 'error' && updater.error.value"
                  class="text-sm text-red-500"
                >
                  {{ updater.error.value }}
                </div>

                

              </div>
              <!-- Update details -->
              <div
                v-if="updater.state.value === 'available' && updater.info.value"
                class="rounded-lg border border-surface-200 dark:border-surface-700
                      bg-surface-50 dark:bg-surface-900 p-3 text-sm"
              >
                <div class="flex flex-col gap-1">
                  <div class="font-medium text-surface-800 dark:text-surface-100">
                    Version {{ updater.info.value.version ?? "-" }}
                  </div>

                  <div
                    v-if="pubDateText"
                    class="text-xs text-surface-500"
                  >
                    Released: {{ pubDateText }}
                  </div>

                  <div
                    v-if="updater.info.value.notes"
                    class="mt-2 whitespace-pre-wrap text-surface-600 dark:text-surface-300"
                  >
                    {{ updater.info.value.notes }}
                  </div>
                </div>
              </div>
              <!-- Actions -->
              <div class="flex flex-col gap-3 mt-2">

                <!-- Install -->
                <Button
                  v-if="updater.state.value === 'available'"
                  label="Download & Install"
                  icon="pi pi-download"
                  severity="primary"
                  :disabled="updater.isDownloading.value"
                  @click="updater.downloadAndInstall"
                />

                <!-- Downloading -->
                <div v-if="updater.state.value === 'downloading'" class="flex flex-col gap-2">
                  <ProgressBar :value="updater.progressPercent.value" />
                  <div class="text-xs text-surface-500">
                    {{ Math.floor(updater.progressPercent.value) }} %
                  </div>
                </div>

                <!-- Restart -->
                <div
                  v-if="updater.state.value === 'ready'"
                  class="text-sm text-surface-600 dark:text-surface-300"
                >
                  Please restart NetDia to apply the update.
                </div>

                <!-- Microsoft Store -->
                <Button
                  v-if="updater.state.value === 'store'"
                  label="Open Microsoft Store"
                  icon="pi pi-external-link"
                  outlined
                  @click="openStore"
                />
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
