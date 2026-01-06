import { ref, provide, onMounted, onBeforeUnmount } from "vue";
import { PrimeIcons } from "@primevue/core/api";
import LogoLight from "../assets/logo-light.png";
import LogoDark from "../assets/logo-dark.png";
import { config } from "../config";

type ThemeMode = "system" | "light" | "dark";

export function useTheme() {
  const STORAGE_KEY = "theme"; // "system" | "light" | "dark"

  // Current theme state (system/light/dark)
  const themeMode = ref<ThemeMode>("system");
  // Actual applied theme (darkmode: true/false)
  const darkmode = ref<boolean>(false);

  // Current theme icon for toggle button (Sun/Moon)
  const currentThemeIcon = ref<string>(PrimeIcons.SUN);
  // Current mode icon for display (Desktop/Sun/Moon)
  const currentModeIcon = ref<string>(PrimeIcons.DESKTOP);

  const currentTheme = ref<string>(config.LIGHT_THEME_NAME);
  const currentLogoFile = ref<string>(LogoLight);

  // Watch for OS theme changes
  let mql: MediaQueryList | null = null;
  const isOsDark = () => window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;

  const applyDomDark = (on: boolean) => {
    const el = document.documentElement; // <html>
    if (!el) return;
    el.classList.toggle("app-dark", on);
  };

  const reflectStateToUi = (isDark: boolean) => {
    darkmode.value = isDark;
    currentTheme.value = isDark ? config.DARK_THEME_NAME : config.LIGHT_THEME_NAME;
    currentThemeIcon.value = isDark ? PrimeIcons.MOON : PrimeIcons.SUN;
    currentLogoFile.value = isDark ? LogoDark : LogoLight;

    // Current mode icon for display (Desktop/Sun/Moon)
    currentModeIcon.value =
      themeMode.value === "system"
        ? PrimeIcons.DESKTOP
        : isDark
        ? PrimeIcons.MOON
        : PrimeIcons.SUN;
  };

  const persistMode = () => {
    localStorage.setItem(STORAGE_KEY, themeMode.value);
  };

  /** ---- Public API ---- */

  // Quick toggle (invert current appearance) * Mode is fixed to light/dark
  const toggleTheme = () => {
    const nextDark = !darkmode.value;
    applyDomDark(nextDark);
    themeMode.value = nextDark ? "dark" : "light";
    reflectStateToUi(nextDark);
    persistMode();
  };

  const setDarkTheme = () => {
    themeMode.value = "dark";
    applyDomDark(true);
    reflectStateToUi(true);
    persistMode();
  };

  const setLightTheme = () => {
    themeMode.value = "light";
    applyDomDark(false);
    reflectStateToUi(false);
    persistMode();
  };

  const setSystemTheme = () => {
    themeMode.value = "system";
    const dark = isOsDark();
    applyDomDark(dark);
    reflectStateToUi(dark);
    persistMode();
  };

  const initFromStorage = () => {
    const saved = (localStorage.getItem(STORAGE_KEY) as ThemeMode) || null;
    if (!saved) {
      themeMode.value = "dark";
      applyDomDark(true);
      reflectStateToUi(true);
      persistMode();
      return;
    }

    themeMode.value = saved;

    if (themeMode.value === "system") {
      const dark = isOsDark();
      applyDomDark(dark);
      reflectStateToUi(dark);
    } else if (themeMode.value === "dark") {
      applyDomDark(true);
      reflectStateToUi(true);
    } else {
      applyDomDark(false);
      reflectStateToUi(false);
    }
  };

  const handleOsSchemeChange = (e: MediaQueryListEvent) => {
    if (themeMode.value === "system") {
      applyDomDark(e.matches);
      reflectStateToUi(e.matches);
    }
  };

  onMounted(() => {
    initFromStorage();
    if (window.matchMedia) {
      mql = window.matchMedia("(prefers-color-scheme: dark)");
      // For legacy Safari support, care for both addEventListener and addListener
      if ("addEventListener" in mql) {
        mql.addEventListener("change", handleOsSchemeChange);
      } else {
        // @ts-expect-error legacy
        mql.addListener(handleOsSchemeChange);
      }
    }
  });

  onBeforeUnmount(() => {
    if (mql) {
      if ("removeEventListener" in mql) {
        mql.removeEventListener("change", handleOsSchemeChange);
      } else {
        // @ts-expect-error legacy
        mql.removeListener(handleOsSchemeChange);
      }
    }
  });

  // Provide current theme to descendants
  provide("theme", currentTheme);

  return {
    themeMode,           
    darkmode,            
    currentTheme,        
    currentThemeIcon,    
    currentModeIcon,     
    currentLogoFile,
    toggleTheme,
    setSystemTheme,
    setDarkTheme,
    setLightTheme,
  };
}
