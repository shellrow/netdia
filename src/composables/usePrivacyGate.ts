import { useUiPreferences } from "./useUiPreferences";

const MASK_STR = "**hidden**";

const { publicIpVisible, hostnameVisible, patchUiPreferences } = useUiPreferences();

// Toggle visibility
function togglePublicIp() {
  void patchUiPreferences({ public_ip_visible: !publicIpVisible.value });
}

function toggleHostname() {
  void patchUiPreferences({ hostname_visible: !hostnameVisible.value });
}

// Gate function - only returns actual value if visible, otherwise mask
function pubIpGate<T extends string | number | null | undefined>(
  value: T,
  opts?: { mask?: string }
): string {
  const mask = opts?.mask ?? MASK_STR;
  if (!value) return "-";
  return publicIpVisible.value ? String(value) : mask;
}

// Gate for compound values (arrays, objects)
function pubIpGateList(values?: string[] | null, opts?: { mask?: string }): string {
  if (!values || values.length === 0) return "-";
  return publicIpVisible.value ? values.join(", ") : (opts?.mask ?? MASK_STR);
}

function hostnameGate<T extends string | null | undefined>(
  value: T,
  opts?: { mask?: string }
): string {
  const mask = opts?.mask ?? MASK_STR;
  if (!value) return "-";
  return hostnameVisible.value ? String(value) : mask;
}

export function usePrivacyGate() {
  return {
    publicIpVisible,
    togglePublicIp,
    pubIpGate,
    pubIpGateList,
    hostnameVisible,
    toggleHostname,
    hostnameGate,
  };
}
