import { ref } from "vue";

const LS_PUBLIC_IP_VISIBLE = "nd:privacy:public_ip_visible";
const LS_HOSTNAME_VISIBLE = "nd:privacy:hostname_visible";
const MASK_STR = "**hidden**";

// Persistent visibility flag
const publicIpVisible = ref<boolean>(
  (localStorage.getItem(LS_PUBLIC_IP_VISIBLE) ?? "1") === "1"
);

const hostnameVisible = ref<boolean>(
  (localStorage.getItem(LS_HOSTNAME_VISIBLE) ?? "1") === "1"
);

// Toggle visibility
function togglePublicIp() {
  publicIpVisible.value = !publicIpVisible.value;
  localStorage.setItem(LS_PUBLIC_IP_VISIBLE, publicIpVisible.value ? "1" : "0");
}

function toggleHostname() {
  hostnameVisible.value = !hostnameVisible.value;
  localStorage.setItem(LS_HOSTNAME_VISIBLE, hostnameVisible.value ? "1" : "0");
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
