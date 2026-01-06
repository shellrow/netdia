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

// Components
import StyleClass from 'primevue/styleclass';
import Avatar from 'primevue/avatar';
import Button from 'primevue/button';
import IconField from 'primevue/iconfield';
import InputIcon from 'primevue/inputicon';
import InputText from 'primevue/inputtext';
import InputNumber from 'primevue/inputnumber';
import InputGroup from 'primevue/inputgroup';
import InputGroupAddon from 'primevue/inputgroupaddon';
import Badge from 'primevue/badge';
import OverlayBadge from 'primevue/overlaybadge';
import Divider from 'primevue/divider';
import MultiSelect from 'primevue/multiselect';
import DataTable from 'primevue/datatable';
import Column from 'primevue/column';
import ColumnGroup from 'primevue/columngroup';
import Row from 'primevue/row';
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
import Textarea from 'primevue/textarea';
import Chart from 'primevue/chart';

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
      localStorage.setItem("np:config", JSON.stringify(cfg));
      localStorage.setItem("np:set:theme", cfg.theme);
      localStorage.setItem("np:set:refresh_ms", String(cfg.refresh_interval_ms));
      localStorage.setItem("np:set:bps_unit", cfg.data_unit);
      localStorage.setItem("np:set:autostart", cfg.startup ? "1" : "0");
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

app.component('Avatar', Avatar);
app.component('Button', Button);
app.component('IconField', IconField);
app.component('InputIcon', InputIcon);
app.component('InputText', InputText);
app.component('InputNumber', InputNumber);
app.component('InputGroup', InputGroup);
app.component('InputGroupAddon', InputGroupAddon);
app.component('Badge', Badge);
app.component('Tag', Tag);
app.component('OverlayBadge', OverlayBadge);
app.component('Divider', Divider);
app.component('MultiSelect', MultiSelect);
app.component('DataTable', DataTable);
app.component('Column', Column);
app.component('ColumnGroup', ColumnGroup);
app.component('Row', Row);
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
app.component('Textarea', Textarea);
app.component('Chart', Chart);

app.directive('tooltip', Tooltip);
app.directive('styleclass', StyleClass);

app.mount('#app');
