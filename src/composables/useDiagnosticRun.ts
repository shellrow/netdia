import { onBeforeUnmount, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { createDiagnosticRun } from "../utils/diagnosticRun";

export function useDiagnosticRun(kind: string, identity: Ref<string | null>) {
  const run = createDiagnosticRun(kind, identity, invoke);
  onBeforeUnmount(() => {
    void run.dispose().catch((error) => {
      console.error("Failed to cancel diagnostic when leaving the page", error);
    });
  });
  return run;
}
