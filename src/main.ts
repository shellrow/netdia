import { createApp } from "vue";
import App from "./App.vue";
import { invoke } from "@tauri-apps/api/core";
import router from './router';
import PrimeVue from 'primevue/config';
import Aura from '@primeuix/themes/aura';
import { definePreset } from '@primeuix/themes';
import './style.css';
import 'primeicons/primeicons.css';
import type { AppConfig } from "./types/config";
import { STORAGE_KEYS } from "./constants/storage";

// Components
import StyleClass from 'primevue/styleclass';
import Button from 'primevue/button';
import InputText from 'primevue/inputtext';
import InputNumber from 'primevue/inputnumber';
import InputGroup from 'primevue/inputgroup';
import InputGroupAddon from 'primevue/inputgroupaddon';
import Badge from 'primevue/badge';
import OverlayBadge from 'primevue/overlaybadge';
import Divider from 'primevue/divider';
import MultiSelect from 'primevue/multiselect';
import Tag from 'primevue/tag';
import ScrollPanel from 'primevue/scrollpanel';
import Dialog from 'primevue/dialog';
import Chip from 'primevue/chip';
import Select from 'primevue/select';
import Tooltip from 'primevue/tooltip';
import Card from 'primevue/card';
import RadioButton from 'primevue/radiobutton';
import RadioButtonGroup from 'primevue/radiobuttongroup';
import ToggleSwitch from 'primevue/toggleswitch';
import ProgressBar from 'primevue/progressbar';
import Checkbox from 'primevue/checkbox';
import CheckboxGroup from 'primevue/checkboxgroup';
import SelectButton from 'primevue/selectbutton';

const ThemePreset = definePreset(Aura, {
    semantic: {
        primary: {
            50: '{indigo.50}',
            100: '{indigo.100}',
            200: '{indigo.200}',
            300: '{indigo.300}',
            400: '{indigo.400}',
            500: '{indigo.500}',
            600: '{indigo.600}',
            700: '{indigo.700}',
            800: '{indigo.800}',
            900: '{indigo.900}',
            950: '{indigo.950}'
        },
        colorScheme: {
            light: {
                semantic: {
                    highlight: {
                        background: '{primary.50}',
                        color: '{primary.700}',
                    }
                }
            },
            dark: {
                semantic: {
                    highlight: {
                        background: '{primary.200}',
                        color: '{primary.900}',
                    }
                }
            }
        }
    }
});

async function initConfigFromTauri() {
  try {
    const cfg: AppConfig = await invoke("get_config");
    if (cfg) {
      localStorage.setItem(STORAGE_KEYS.CONFIG, JSON.stringify(cfg));
      localStorage.setItem(STORAGE_KEYS.THEME, cfg.theme);
      localStorage.setItem(STORAGE_KEYS.REFRESH_INTERVAL_MS, String(cfg.refresh_interval_ms));
      localStorage.setItem(STORAGE_KEYS.BPS_UNIT, cfg.data_unit);
      localStorage.setItem(STORAGE_KEYS.AUTOSTART, cfg.startup ? "1" : "0");
      localStorage.setItem(STORAGE_KEYS.AUTO_INTERNET_CHECK, cfg.auto_internet_check ? "1" : "0");
      localStorage.setItem(STORAGE_KEYS.AUTO_INTERNET_CHECK_INTERVAL_S, String(cfg.auto_internet_check_interval_s));
    }
  } catch (e) {
    console.error("Failed to load config from Tauri:", e);
  }
}

;(async () => {
  await initConfigFromTauri()
})();

const app = createApp(App);
app.use(router);
app.use(PrimeVue, {
    theme: {
        preset: ThemePreset,
        options: {
            darkModeSelector: '.app-dark',
        }
    }
});

app.component('Button', Button);
app.component('InputText', InputText);
app.component('InputNumber', InputNumber);
app.component('InputGroup', InputGroup);
app.component('InputGroupAddon', InputGroupAddon);
app.component('Tag', Tag);
app.component('Badge', Badge);
app.component('OverlayBadge', OverlayBadge);
app.component('Divider', Divider);
app.component('MultiSelect', MultiSelect);
app.component('ScrollPanel', ScrollPanel);
app.component('Dialog', Dialog);
app.component('Chip', Chip);
app.component('Select', Select);
app.component('Card', Card);
app.component('RadioButton', RadioButton);
app.component('RadioButtonGroup', RadioButtonGroup);
app.component('ToggleSwitch', ToggleSwitch);
app.component('ProgressBar', ProgressBar);
app.component('Checkbox', Checkbox);
app.component('CheckboxGroup', CheckboxGroup);
app.component('SelectButton', SelectButton);

app.directive('tooltip', Tooltip);
app.directive('styleclass', StyleClass);

app.mount('#app');
