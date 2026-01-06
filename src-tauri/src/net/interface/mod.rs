use netdev::Interface;

pub fn list_interfaces() -> Vec<Interface> {
    netdev::get_interfaces()
}

pub fn get_display_name(iface: &Interface) -> String {
    // On Windows, use the friendly name if available
    #[cfg(target_os = "windows")]
    {
        if let Some(friendly_name) = &iface.friendly_name {
            return friendly_name.clone();
        }
    }

    // On macOS:
    // Some interfaces provide a friendly name that already includes the
    // interface name (e.g., "Ethernet Adapter (en4)"). In that case we use it as-is.
    // Otherwise we format it as "{friendly_name} ({name})" to keep things
    // consistent while avoiding duplicate "(name)" suffixes.
    #[cfg(target_os = "macos")]
    {
        if let Some(friendly_name) = &iface.friendly_name {
            if friendly_name.contains(&iface.name) {
                return friendly_name.clone();
            } else {
                return format!("{} ({})", friendly_name, iface.name);
            }
        }
    }

    // Fallback to name
    iface.name.clone()
}
