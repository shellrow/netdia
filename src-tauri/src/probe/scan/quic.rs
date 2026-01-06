use anyhow::Result;
use futures::{stream, StreamExt};
use rand::{seq::SliceRandom, thread_rng};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::model::endpoint::Endpoint;
use crate::model::scan::{PortScanReport, PortScanSample, PortScanSetting, PortState};
use crate::probe::scan::expand_ports;
use crate::probe::scan::progress::ThrottledProgress;
use crate::probe::scan::tuner::ports_concurrency;
use crate::probe::service::{ServiceDetector, ServiceProbeConfig};

pub async fn port_scan(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: PortScanSetting,
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

    let hostname_opt = setting.hostname.clone();

    // Create tasks for each port and collect results as they complete.
    let mut tasks = stream::iter(ports.into_iter())
        .map(|port| {
            let app = app.clone();
            let progress = progress.clone();
            let hostname_opt = hostname_opt.clone();

            async move {
                let family = if ip.is_ipv4() {
                    crate::socket::SocketFamily::IPV4
                } else {
                    crate::socket::SocketFamily::IPV6
                };

                let quic_cfg = crate::socket::quic::QuicConfig {
                    skip_verify: true,
                    alpn: vec![b"h3".to_vec(), b"hq-29".to_vec(), b"hq-interop".to_vec()],
                    family,
                };

                let (state, rtt_ms, msg) =
                    match crate::socket::quic::AsyncQuicSocket::from_config(&quic_cfg) {
                        Ok(ep) => {
                            let server_name =
                                hostname_opt.clone().unwrap_or_else(|| ip.to_string());
                            let start = Instant::now();
                            match ep
                                .connect_timeout(&SocketAddr::new(ip, port), &server_name, timeout)
                                .await
                            {
                                Ok(conn) => {
                                    conn.close(0u32.into(), b"done");
                                    (
                                        PortState::Open,
                                        Some(start.elapsed().as_millis() as u64),
                                        None,
                                    )
                                }
                                Err(e) => {
                                    let st = if let Some(ioe) = e.downcast_ref::<std::io::Error>() {
                                        if ioe.kind() == std::io::ErrorKind::TimedOut {
                                            PortState::Filtered
                                        } else {
                                            PortState::Closed
                                        }
                                    } else {
                                        PortState::Closed
                                    };
                                    (st, None, Some(e.to_string()))
                                }
                            }
                        }
                        Err(e) => (
                            PortState::Filtered,
                            None,
                            Some(format!("quic endpoint error: {}", e)),
                        ),
                    };

                let (done, should_emit) = progress.on_advance();

                let sample = PortScanSample {
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
                    let _ = app.emit("portscan:progress", (done, total));
                }

                sample
            }
        })
        .buffer_unordered(ports_concurrency());

    // Collect only Open samples
    let mut open_samples: Vec<PortScanSample> = Vec::new();
    let udp_service_db = ndb_udp_service::UdpServiceDb::bundled();
    while let Some(mut sample) = tasks.next().await {
        if sample.state == PortState::Open {
            sample.service_name = udp_service_db
                .get(sample.port)
                .map(|entry| entry.name.clone());
            open_samples.push(sample);
        }
    }

    open_samples.sort_by_key(|s| s.port);

    // Service detection
    if setting.service_detection && !open_samples.is_empty() {
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
                transport: crate::model::endpoint::TransportProtocol::Quic,
            });
        }
        let active_endpoints: Vec<Endpoint> = vec![endpoint];
        let service_result = detector.run_service_detection(active_endpoints).await?;
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
