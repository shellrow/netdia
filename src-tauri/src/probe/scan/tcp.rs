use anyhow::Result;
use futures::{stream, StreamExt};
use rand::{seq::SliceRandom, thread_rng};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::model::endpoint::Endpoint;
use crate::model::scan::{
    PortScanCancelledPayload, PortScanProgressPayload, PortScanReport, PortScanSample,
    PortScanSetting, PortState,
};
use crate::probe::scan::expand_ports;
use crate::probe::scan::progress::ThrottledProgress;
use crate::probe::scan::tuner::ports_concurrency;
use crate::probe::service::{ServiceDetector, ServiceProbeConfig};

pub async fn port_scan(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: PortScanSetting,
    token: CancellationToken,
) -> Result<PortScanReport> {
    let mut ports = expand_ports(&setting.target_ports_preset, &setting.user_ports);
    if !setting.ordered {
        ports.shuffle(&mut thread_rng());
    }

    let app = app.clone();
    let ip = setting.ip_addr;
    let timeout = Duration::from_millis(setting.timeout_ms);

    let total = ports.len() as u32;
    let progress = Arc::new(ThrottledProgress::new(total));

    // Create tasks for each port and collect results as they complete.
    let mut tasks = stream::iter(ports.into_iter())
        .map(|port| {
            let app = app.clone();
            let progress = progress.clone();
            let token = token.clone();
            async move {
                if token.is_cancelled() {
                    return None;
                }
                let cfg = if ip.is_ipv4() {
                    crate::socket::tcp::TcpConfig::v4_stream()
                } else {
                    crate::socket::tcp::TcpConfig::v6_stream()
                };

                let sock_addr = SocketAddr::new(ip, port);
                let sock = match crate::socket::tcp::AsyncTcpSocket::from_config(&cfg) {
                    Ok(s) => s,
                    Err(e) => {
                        let (done, should_emit) = progress.on_advance();

                        if should_emit {
                            let _ = app.emit(
                                "portscan:progress",
                                PortScanProgressPayload {
                                    run_id: run_id.to_string(),
                                    done,
                                    total,
                                },
                            );
                        }

                        return Some(PortScanSample {
                            run_id: run_id.to_string(),
                            ip_addr: ip,
                            port,
                            state: PortState::Filtered,
                            rtt_ms: None,
                            message: Some(format!("tcp socket error: {}", e)),
                            service_name: None,
                            service_info: None,
                            done,
                            total,
                        });
                    }
                };

                let start = Instant::now();

                let connect_res = tokio::select! {
                    _ = token.cancelled() => {
                        return None;
                    }
                    r = sock.connect_timeout(sock_addr, timeout) => r,
                };

                let (state, rtt_ms, msg) = match connect_res {
                    Ok(stream) => {
                        drop(stream);
                        (
                            PortState::Open,
                            Some(start.elapsed().as_millis() as u64),
                            None,
                        )
                    }
                    Err(e) => {
                        use std::io::ErrorKind as E;
                        let st = match e.kind() {
                            E::TimedOut => PortState::Filtered,
                            E::ConnectionRefused | E::ConnectionReset | E::NotConnected => {
                                PortState::Closed
                            }
                            E::NetworkUnreachable | E::HostUnreachable | E::AddrNotAvailable => {
                                PortState::Filtered
                            }
                            _ => PortState::Closed,
                        };
                        (st, None, Some(e.to_string()))
                    }
                };

                let (done, should_emit) = progress.on_advance();

                let sample = PortScanSample {
                    run_id: run_id.to_string(),
                    ip_addr: ip,
                    port,
                    state,
                    rtt_ms,
                    message: msg,
                    service_name: None,
                    service_info: None,
                    done,
                    total,
                };

                // Open port: emit detailed info
                if sample.state == PortState::Open {
                    let _ = app.emit("portscan:open", sample.clone());
                }

                // Progress event
                if should_emit {
                    let _ = app.emit(
                        "portscan:progress",
                        PortScanProgressPayload {
                            run_id: run_id.to_string(),
                            done,
                            total,
                        },
                    );
                }

                Some(sample)
            }
        })
        .buffer_unordered(ports_concurrency());

    // Collect Open results only
    let mut open_samples = Vec::new();
    let tcp_db = ndb_tcp_service::TcpServiceDb::bundled();
    let mut cancelled = false;
    loop {
        let sample = tokio::select! {
            _ = token.cancelled() => {
                cancelled = true;
                break;
            }
            s = tasks.next() => s,
        };

        let Some(sample) = sample else {
            break;
        };

        if let Some(mut sample) = sample {
            if sample.state == PortState::Open {
                if let Some(entry) = tcp_db.get(sample.port) {
                    sample.service_name = Some(entry.name.clone());
                }
                open_samples.push(sample);
            }
        }
    }

    if cancelled || token.is_cancelled() {
        let _ = app.emit(
            "portscan:cancelled",
            PortScanCancelledPayload {
                run_id: run_id.to_string(),
            },
        );
        return Err(anyhow::anyhow!("cancelled"));
    }

    // Sort by port
    open_samples.sort_by_key(|s| s.port);

    // Service detection
    if setting.service_detection && !open_samples.is_empty() && !token.is_cancelled() {
        let _ = app.emit("portscan:service_detection_start", run_id.to_string());
        let service_probe_setting = ServiceProbeConfig {
            timeout: Duration::from_secs(2),
            max_concurrency: 100,
            max_read_size: 1024 * 1024,
            sni: true,
            skip_cert_verify: true,
        };
        let detector = ServiceDetector::new(service_probe_setting);
        let mut endpoint = Endpoint::new(ip);
        endpoint.hostname = setting.hostname.clone();
        for sample in &open_samples {
            endpoint.upsert_port(crate::model::endpoint::Port {
                number: sample.port,
                transport: crate::model::endpoint::TransportProtocol::Tcp,
            });
        }
        let active_endpoints: Vec<Endpoint> = vec![endpoint];
        let mut service_cancelled = false;
        let service_result = tokio::select! {
            _ = token.cancelled() => {
                service_cancelled = true;
                None
            }
            r = detector.run_service_detection(active_endpoints) => Some(r?),
        };

        if service_cancelled || token.is_cancelled() {
            let _ = app.emit(
                "portscan:cancelled",
                PortScanCancelledPayload {
                    run_id: run_id.to_string(),
                },
            );
            return Err(anyhow::anyhow!("cancelled"));
        } else if let Some(service_result) = service_result {
            for sample in &mut open_samples {
                if let Some(res) = service_result
                    .results
                    .iter()
                    .find(|r| r.port == sample.port)
                {
                    sample.service_info = Some(res.service_info.clone());
                }
            }
            let _ = app.emit("portscan:service_detection_done", run_id.to_string());
        }
    }

    let report = PortScanReport {
        run_id: run_id.to_string(),
        ip_addr: setting.ip_addr,
        hostname: setting.hostname.clone(),
        protocol: setting.protocol,
        samples: open_samples,
    };

    let _ = app.emit("portscan:done", report.clone());
    Ok(report)
}
