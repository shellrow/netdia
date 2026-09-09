import { onBeforeUnmount, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { createListenerScope } from "../utils/listenerScope";

export function useDiagnosticListeners() {
  const scope = createListenerScope(listen);
  const listenersReady = ref(false);
  onBeforeUnmount(() => {
    listenersReady.value = false;
    scope.dispose();
  });
  return { ...scope, listenersReady };
}
