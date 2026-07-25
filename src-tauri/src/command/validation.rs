use crate::{
    config::AppConfig,
    model::{
        ping::{PingProtocol, PingSetting},
        scan::{HostScanRequest, PortScanSetting, TargetPortsPreset},
        speedtest::SpeedtestSetting,
        trace::TracerouteSetting,
    },
};

const MAX_HOST_LENGTH: usize = 253;
const MAX_HOST_SCAN_TARGETS: usize = 65_536;
const MAX_HOST_SCAN_CONCURRENCY: usize = 1_024;
const MAX_PAYLOAD_LENGTH: usize = 1_400;
const MAX_SPEEDTEST_BYTES: u64 = 104_857_600;

pub fn validate_host(value: &str, field: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    if value.len() > MAX_HOST_LENGTH {
        return Err(format!(
            "{field} must be at most {MAX_HOST_LENGTH} characters"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} contains control characters"));
    }
    Ok(())
}

pub fn validate_config(config: &AppConfig) -> Result<(), String> {
    if !(100..=60_000).contains(&config.refresh_interval_ms) {
        return Err("refresh_interval_ms must be between 100 and 60000".to_string());
    }
    if !matches!(config.theme.as_str(), "dark" | "light" | "system") {
        return Err("theme must be dark, light, or system".to_string());
    }
    if !matches!(config.data_unit.as_str(), "bits" | "bytes") {
        return Err("data_unit must be bits or bytes".to_string());
    }
    if !(30..=3_600).contains(&config.auto_internet_check_interval_s) {
        return Err("auto_internet_check_interval_s must be between 30 and 3600".to_string());
    }
    Ok(())
}

pub fn validate_ping(setting: &PingSetting) -> Result<(), String> {
    if setting.hop_limit == 0 {
        return Err("hop_limit must be between 1 and 255".to_string());
    }
    if !(1..=1_000).contains(&setting.count) {
        return Err("count must be between 1 and 1000".to_string());
    }
    if !(100..=60_000).contains(&setting.timeout_ms) {
        return Err("timeout_ms must be between 100 and 60000".to_string());
    }
    if !(100..=60_000).contains(&setting.send_rate_ms) {
        return Err("send_rate_ms must be between 100 and 60000".to_string());
    }
    if !matches!(setting.protocol, PingProtocol::Icmp) && setting.port.is_none() {
        return Err("port is required for the selected protocol".to_string());
    }
    if let Some(hostname) = setting.hostname.as_deref() {
        validate_host(hostname, "hostname")?;
    }
    Ok(())
}

pub fn validate_traceroute(setting: &TracerouteSetting) -> Result<(), String> {
    if !(1..=64).contains(&setting.max_hops) {
        return Err("max_hops must be between 1 and 64".to_string());
    }
    if !(1..=5).contains(&setting.tries_per_hop) {
        return Err("tries_per_hop must be between 1 and 5".to_string());
    }
    if !(100..=10_000).contains(&setting.timeout_ms) {
        return Err("timeout_ms must be between 100 and 10000".to_string());
    }
    if let Some(hostname) = setting.hostname.as_deref() {
        validate_host(hostname, "hostname")?;
    }
    Ok(())
}

pub fn validate_port_scan(setting: &PortScanSetting) -> Result<(), String> {
    if !(50..=60_000).contains(&setting.timeout_ms) {
        return Err("timeout_ms must be between 50 and 60000".to_string());
    }
    if matches!(setting.target_ports_preset, TargetPortsPreset::Custom)
        && setting.user_ports.is_empty()
    {
        return Err("at least one custom port is required".to_string());
    }
    if setting.user_ports.contains(&0) {
        return Err("port 0 is not a valid scan target".to_string());
    }
    if let Some(hostname) = setting.hostname.as_deref() {
        validate_host(hostname, "hostname")?;
    }
    Ok(())
}

pub fn validate_host_scan(setting: &HostScanRequest) -> Result<(), String> {
    if setting.targets.is_empty() {
        return Err("at least one target is required".to_string());
    }
    if setting.targets.len() > MAX_HOST_SCAN_TARGETS {
        return Err(format!(
            "target count must not exceed {MAX_HOST_SCAN_TARGETS}"
        ));
    }
    for target in &setting.targets {
        validate_host(target, "target")?;
    }
    if setting.hop_limit == 0 {
        return Err("hop_limit must be between 1 and 255".to_string());
    }
    if !(100..=60_000).contains(&setting.timeout_ms) {
        return Err("timeout_ms must be between 100 and 60000".to_string());
    }
    if !(1..=100).contains(&setting.count) {
        return Err("count must be between 1 and 100".to_string());
    }
    if let Some(concurrency) = setting.concurrency {
        if !(1..=MAX_HOST_SCAN_CONCURRENCY).contains(&concurrency) {
            return Err(format!(
                "concurrency must be between 1 and {MAX_HOST_SCAN_CONCURRENCY}"
            ));
        }
    }
    if setting
        .payload
        .as_ref()
        .is_some_and(|payload| payload.len() > MAX_PAYLOAD_LENGTH)
    {
        return Err(format!(
            "payload must be at most {MAX_PAYLOAD_LENGTH} bytes"
        ));
    }
    Ok(())
}

pub fn validate_speedtest(setting: &SpeedtestSetting) -> Result<(), String> {
    if !(102_400..=MAX_SPEEDTEST_BYTES).contains(&setting.target_bytes) {
        return Err(format!(
            "target_bytes must be between 102400 and {MAX_SPEEDTEST_BYTES}"
        ));
    }
    if setting
        .max_duration_ms
        .is_some_and(|duration| !(1_000..=60_000).contains(&duration))
    {
        return Err("max_duration_ms must be between 1000 and 60000".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AppConfig,
        model::{
            ping::PingProtocol,
            scan::{PortScanProtocol, TargetPortsPreset},
            speedtest::{SpeedtestDirection, SpeedtestType},
            trace::TraceProtocol,
        },
    };
    use std::net::{IpAddr, Ipv4Addr};

    fn ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    }

    #[test]
    fn rejects_invalid_config_values() {
        let config = AppConfig {
            theme: "neon".to_string(),
            ..AppConfig::default()
        };
        assert_eq!(
            validate_config(&config).unwrap_err(),
            "theme must be dark, light, or system"
        );
    }

    #[test]
    fn rejects_control_characters_in_hosts() {
        assert!(validate_host("example.com\nother", "hostname").is_err());
    }

    #[test]
    fn rejects_ping_without_required_port() {
        let setting = PingSetting {
            ip_addr: ip(),
            hostname: None,
            port: None,
            hop_limit: 64,
            protocol: PingProtocol::Tcp,
            count: 4,
            timeout_ms: 2_000,
            send_rate_ms: 1_000,
        };
        assert!(validate_ping(&setting).is_err());
    }

    #[test]
    fn accepts_valid_traceroute() {
        let setting = TracerouteSetting {
            ip_addr: ip(),
            hostname: Some("localhost".to_string()),
            max_hops: 30,
            tries_per_hop: 2,
            timeout_ms: 2_000,
            protocol: TraceProtocol::Icmp,
        };
        assert!(validate_traceroute(&setting).is_ok());
    }

    #[test]
    fn rejects_empty_custom_port_scan() {
        let setting = PortScanSetting {
            ip_addr: ip(),
            hostname: None,
            target_ports_preset: TargetPortsPreset::Custom,
            user_ports: Vec::new(),
            protocol: PortScanProtocol::Tcp,
            timeout_ms: 1_500,
            ordered: false,
            service_detection: false,
        };
        assert!(validate_port_scan(&setting).is_err());
    }

    #[test]
    fn rejects_excessive_host_scan_concurrency() {
        let setting = HostScanRequest {
            targets: vec!["127.0.0.1".to_string()],
            hop_limit: 64,
            timeout_ms: 1_000,
            count: 1,
            payload: None,
            ordered: false,
            concurrency: Some(MAX_HOST_SCAN_CONCURRENCY + 1),
        };
        assert!(validate_host_scan(&setting).is_err());
    }

    #[test]
    fn rejects_unbounded_speedtest() {
        let setting = SpeedtestSetting {
            direction: SpeedtestDirection::Download,
            test_type: SpeedtestType::ByteStream,
            target_bytes: MAX_SPEEDTEST_BYTES + 1,
            max_duration_ms: None,
        };
        assert!(validate_speedtest(&setting).is_err());
    }
}
