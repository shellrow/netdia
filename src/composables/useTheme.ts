import { ref } from "vue";
import { PrimeIcons } from "@primevue/core/api";
import LogoLight from "../assets/logo-light.png";
import LogoDark from "../assets/logo-dark.png";
import { config } from "../config";

export type ThemeMode = "system" | "light" | "dark";

const themeMode = ref<ThemeMode>("dark");
const darkmode = ref<boolean>(true);
const currentThemeIcon = ref<string>(PrimeIcons.MOON);
const currentModeIcon = ref<string>(PrimeIcons.MOON);
const currentTheme = ref<string>(config.DARK_THEME_NAME);
const currentLogoFile = ref<string>(LogoDark);

let mql: MediaQueryList | null = null;
let watcherAttached = false;

const isOsDark = () =>
  window.matchMedia &&
  window.matchMedia("(prefers-color-scheme: dark)").matches;

const applyDomDark = (on: boolean) => {
  const el = document.documentElement;
  if (!el) return;
  el.classList.toggle("app-dark", on);
};

const reflectStateToUi = (isDark: boolean) => {
  darkmode.value = isDark;
  currentTheme.value = isDark ? config.DARK_THEME_NAME : config.LIGHT_THEME_NAME;
  currentThemeIcon.value = isDark ? PrimeIcons.MOON : PrimeIcons.SUN;
  currentLogoFile.value = isDark ? LogoDark : LogoLight;
  currentModeIcon.value =
    themeMode.value === "system"
      ? PrimeIcons.DESKTOP
      : isDark
        ? PrimeIcons.MOON
        : PrimeIcons.SUN;
};

function applyThemeMode(mode: ThemeMode) {
  themeMode.value = mode;

  if (mode === "system") {
    const dark = isOsDark();
    applyDomDark(dark);
    reflectStateToUi(dark);
    return;
  }

  const dark = mode === "dark";
  applyDomDark(dark);
  reflectStateToUi(dark);
}

function handleOsSchemeChange(e: MediaQueryListEvent) {
  if (themeMode.value === "system") {
    applyDomDark(e.matches);
    reflectStateToUi(e.matches);
  }
}

function ensureOsWatcher() {
  if (watcherAttached || !window.matchMedia) return;

  mql = window.matchMedia("(prefers-color-scheme: dark)");
  if ("addEventListener" in mql) {
    mql.addEventListener("change", handleOsSchemeChange);
  } else {
    // @ts-expect-error legacy Safari
    mql.addListener(handleOsSchemeChange);
  }
  watcherAttached = true;
}

export function initTheme(mode: ThemeMode = "dark") {
  ensureOsWatcher();
  applyThemeMode(mode);
}

export function useTheme() {
  return {
    themeMode,
    darkmode,
    currentTheme,
    currentThemeIcon,
    currentModeIcon,
    currentLogoFile,
    toggleTheme: () => applyThemeMode(darkmode.value ? "light" : "dark"),
    setSystemTheme: () => applyThemeMode("system"),
    setDarkTheme: () => applyThemeMode("dark"),
    setLightTheme: () => applyThemeMode("light"),
  };
}
