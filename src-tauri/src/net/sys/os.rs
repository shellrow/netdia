use crate::model::{ProxyEnv, SysInfo};

pub fn hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|os| os.into_string().ok())
        .unwrap_or_else(|| "unknown".into())
}

fn normalize_os_name(os_type: &os_info::Type) -> String {
    match os_type {
        os_info::Type::Macos => "macOS".into(),
        other => other.to_string(),
    }
}

pub fn system_info() -> SysInfo {
    let hostname = hostname();
    let info = os_info::get();
    let os_type = normalize_os_name(&info.os_type());
    let os_version = info.version().to_string();
    let edition = info
        .edition()
        .unwrap_or_else(|| "unknown".into())
        .to_string();
    let codename = info
        .codename()
        .unwrap_or_else(|| "unknown".into())
        .to_string();
    let bitness = if cfg!(target_pointer_width = "64") {
        "64-bit"
    } else {
        "32-bit"
    }
    .into();
    let architecture = std::env::consts::ARCH.into();

    let proxy = collect_proxy_env();

    let kernel_version = kernel_version();

    SysInfo {
        hostname,
        os_type,
        os_version,
        kernel_version,
        edition,
        codename,
        bitness,
        architecture,
        proxy,
    }
}

/// Collect proxy environment variables
pub fn collect_proxy_env() -> ProxyEnv {
    // Prefer lowercase, fallback to uppercase
    fn pick(key: &str) -> Option<String> {
        std::env::var(key.to_lowercase())
            .ok()
            .or_else(|| std::env::var(key.to_uppercase()).ok())
            .filter(|s| !s.trim().is_empty())
    }

    ProxyEnv {
        http: pick("http_proxy"),
        https: pick("https_proxy"),
        all: pick("all_proxy"),
        no_proxy: pick("no_proxy"),
    }
}

#[cfg(target_os = "linux")]
/// Linux-specific: get kernel version from /proc/version
fn kernel_version() -> Option<String> {
    if let Ok(contents) = std::fs::read_to_string("/proc/version") {
        let parts: Vec<&str> = contents.split_whitespace().collect();
        if parts.len() >= 3 {
            return Some(format!("{} {} {}", parts[0], parts[1], parts[2]));
        }
    }
    None
}

#[cfg(target_os = "macos")]
/// macOS-specific: get kernel version using `uname`
fn kernel_version() -> Option<String> {
    use libc::utsname;
    use std::ffi::CStr;
    unsafe {
        let mut uts: utsname = std::mem::zeroed();
        if libc::uname(&mut uts) == 0 {
            let ver = CStr::from_ptr(uts.version.as_ptr()).to_string_lossy();
            let ver_short = ver.split(':').next().unwrap_or(&ver);
            return Some(ver_short.trim().to_string());
        }
    }
    return None;
}

#[cfg(target_os = "windows")]
/// Windows-specific: get kernel version using `RtlGetVersion`
fn kernel_version() -> Option<String> {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    unsafe {
        let mut info = OSVERSIONINFOW {
            dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
            ..std::mem::zeroed()
        };
        let status = RtlGetVersion(&mut info as *mut _ as *mut _);
        if status == 0 {
            let major = info.dwMajorVersion;
            let minor = info.dwMinorVersion;
            let build = info.dwBuildNumber;
            return Some(format!("Windows NT Kernel {major}.{minor}.{build}"));
        }
    }
    return None;
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn kernel_version() -> Option<String> {
    None
}
