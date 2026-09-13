import { computed, ref } from "vue";

const message = ref<string | null>(null);

function normalizeError(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message.trim();
  }
  const text = String(error ?? "").trim();
  return text || "An unexpected error occurred.";
}

export function reportAppError(error: unknown, context?: string) {
  const detail = normalizeError(error);
  message.value = context ? `${context}: ${detail}` : detail;
}

export function clearAppError() {
  message.value = null;
}

export function useAppStatus() {
  return {
    errorMessage: computed(() => message.value),
    hasError: computed(() => message.value !== null),
    reportAppError,
    clearAppError,
  };
}
