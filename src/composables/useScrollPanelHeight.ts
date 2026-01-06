import { ref, onMounted, onBeforeUnmount, nextTick } from "vue";

type Opts = {
  extra?: number;
  gap?: number;
  min?: number;
};

export function useScrollPanelHeight(opts: Opts = {}) {
  const { extra = 28, gap = 12, min = 220 } = opts;

  const wrapRef = ref<HTMLElement | null>(null);
  const toolbarRef = ref<HTMLElement | null>(null);
  const panelHeight = ref("400px");

  let ro: ResizeObserver | null = null;
  let rafId: number | null = null;
  let scheduled = false;

  const calcHeight = () => {
    const wrap = wrapRef.value;
    if (!wrap) return panelHeight.value;
    const cs = getComputedStyle(wrap);
    const padY = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    const inner = wrap.clientHeight - padY;
    const toolbarH = toolbarRef.value?.offsetHeight ?? 0;
    const px = Math.max(min, inner - toolbarH - extra - gap);
    return `${Math.floor(px)}px`;
  };

  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    if (rafId) cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(() => {
      scheduled = false;
      const next = calcHeight();
      if (next !== panelHeight.value) panelHeight.value = next;
    });
  };

  onMounted(async () => {
    await nextTick();
    panelHeight.value = calcHeight();
    ro = new ResizeObserver(() => schedule());
    if (wrapRef.value) ro.observe(wrapRef.value);
    if (toolbarRef.value) ro.observe(toolbarRef.value);
    window.addEventListener("resize", schedule);
  });

  onBeforeUnmount(() => {
    ro?.disconnect();
    if (rafId) cancelAnimationFrame(rafId);
    window.removeEventListener("resize", schedule);
  });

  return { wrapRef, toolbarRef, panelHeight };
}
