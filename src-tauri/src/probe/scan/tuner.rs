use std::sync::LazyLock;

/// Scan performance profile.
/// Controls how aggressively scanning is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanProfile {
    Conservative,
    Balanced,
    Aggressive,
}

impl ScanProfile {
    /// Load profile from environment variable:
    /// NETPULSE_SCAN_PROFILE = conservative | balanced | aggressive
    pub fn from_env() -> Self {
        match std::env::var("NETPULSE_SCAN_PROFILE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "conservative" | "slow" | "low" => Self::Conservative,
            "aggressive" | "fast" | "turbo" => Self::Aggressive,
            _ => Self::Balanced,
        }
    }

    pub fn factor(self) -> f32 {
        match self {
            Self::Conservative => 0.6,
            Self::Balanced => 1.0,
            Self::Aggressive => 1.4,
        }
    }
}

/// Final tuned concurrency settings.
#[derive(Debug, Clone, Copy)]
pub struct ScanConcurrency {
    pub hosts: usize,
    pub ports: usize,
}

/// Global lazy-initialized tuner.
pub static SCAN_CONCURRENCY: LazyLock<ScanConcurrency> = LazyLock::new(|| {
    let profile = ScanProfile::from_env();
    let tuned = calc_scan_concurrency(profile);

    tracing::debug!(
        "Scan concurrency tuned: hosts={}, ports={} (profile={:?}, cpu={})",
        tuned.hosts,
        tuned.ports,
        profile,
        num_cpus::get(),
    );

    tuned
});

/// Compute concurrency values based on CPU count, OS behavior, and scan profile.
pub fn calc_scan_concurrency(profile: ScanProfile) -> ScanConcurrency {
    let cpu = num_cpus::get().max(1);

    // Host scanning concurrency
    let base_hosts = 64 * cpu;

    let os_factor_hosts: f32 = {
        #[cfg(target_os = "windows")]
        {
            0.8
        }
        #[cfg(target_os = "linux")]
        {
            1.0
        }
        #[cfg(target_os = "macos")]
        {
            1.2
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            1.0
        }
    };

    let mut hosts = (base_hosts as f32 * os_factor_hosts * profile.factor()) as usize;
    hosts = hosts.clamp(128, 2048);

    // Port scanning concurrency
    let base_ports = 200 * cpu;

    let os_factor_ports: f32 = {
        #[cfg(target_os = "windows")]
        {
            0.6
        }
        #[cfg(target_os = "linux")]
        {
            1.0
        }
        #[cfg(target_os = "macos")]
        {
            1.3
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            1.0
        }
    };

    let mut ports = (base_ports as f32 * os_factor_ports * profile.factor()) as usize;
    ports = ports.clamp(300, 3000);

    ScanConcurrency { hosts, ports }
}

/// Helpers
pub fn hosts_concurrency() -> usize {
    SCAN_CONCURRENCY.hosts
}

pub fn ports_concurrency() -> usize {
    SCAN_CONCURRENCY.ports
}
