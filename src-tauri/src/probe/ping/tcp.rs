use anyhow::Result;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::model::ping::{
    PingCancelledPayload, PingDonePayload, PingProgressPayload, PingProtocol, PingSample,
    PingSetting, PingStat,
};
use crate::model::probe::{ProbeStatus, ProbeStatusKind};
use crate::socket::tcp::{AsyncTcpSocket, TcpConfig, TcpSocketType};

fn summarize_rtts(rtts_ms: &[u64]) -> (Option<u64>, Option<u64>, Option<u64>) {
    if rtts_ms.is_empty() {
        return (None, None, None);
    }
    let mut min = u64::MAX;
    let mut max = 0u64;
    let mut sum: u128 = 0;
    for &v in rtts_ms {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
        sum += v as u128;
    }
    let avg = (sum / rtts_ms.len() as u128) as u64;
    (Some(min), Some(avg), Some(max))
}

pub async fn tcp_ping(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: PingSetting,
    token: CancellationToken,
) -> Result<PingStat> {
    let port = setting.port.unwrap_or(80);
    let target = SocketAddr::new(setting.ip_addr, port);

    let mut samples = Vec::with_capacity(setting.count as usize);
    let mut rtts_ok = Vec::<u64>::new();

    let mut received = 0u32;

    for seq in 1..=setting.count {
        if token.is_cancelled() {
            let _ = app.emit(
                "ping:cancelled",
                PingCancelledPayload {
                    run_id: run_id.to_string(),
                },
            );
            return Err(anyhow::anyhow!("cancelled"));
        }
        let mut cfg = if setting.ip_addr.is_ipv4() {
            TcpConfig::v4_stream()
        } else {
            TcpConfig::v6_stream()
        };
        cfg.socket_type = TcpSocketType::Stream;
        cfg.nodelay = Some(true);
        cfg.ttl = if setting.ip_addr.is_ipv4() {
            Some(setting.hop_limit as u32)
        } else {
            None
        };
        cfg.hoplimit = if setting.ip_addr.is_ipv6() {
            Some(setting.hop_limit as u32)
        } else {
            None
        };

        let sock = match AsyncTcpSocket::from_config(&cfg) {
            Ok(s) => s,
            Err(e) => {
                let sample = PingSample {
                    seq,
                    ip_addr: setting.ip_addr,
                    hostname: setting.hostname.clone(),
                    port: Some(port),
                    rtt_ms: None,
                    probe_status: ProbeStatus::with_error_message(format!("socket error: {e}")),
                    protocol: PingProtocol::Tcp,
                };

                if sample.rtt_ms.is_some()
                    && matches!(sample.probe_status.kind, ProbeStatusKind::Done)
                {
                    received += 1;
                }

                let transmitted = seq;
                let percent = (seq as f32) * 100.0 / (setting.count as f32);
                let _ = app.emit(
                    "ping:progress",
                    PingProgressPayload {
                        run_id: run_id.to_string(),
                        sample: sample.clone(),
                        transmitted,
                        received,
                        percent,
                    },
                );
                samples.push(sample);

                if seq != setting.count {
                    tokio::time::sleep(Duration::from_millis(setting.send_rate_ms)).await;
                }
                continue;
            }
        };

        let started = Instant::now();
        let mut status = ProbeStatus::new();
        let mut rtt_ms = None;

        // Connect with timeout
        match sock
            .connect_timeout(target, Duration::from_millis(setting.timeout_ms))
            .await
        {
            Ok(mut stream) => {
                // Handshake done. Connected.
                rtt_ms = Some(started.elapsed().as_millis() as u64);
                rtts_ok.push(rtt_ms.unwrap());
                // Close the connection
                let _ = stream.shutdown().await;
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                status = ProbeStatus::with_timeout_message(format!(
                    "timeout (>{}ms)",
                    setting.timeout_ms
                ));
            }
            Err(e) => {
                status = ProbeStatus::with_error_message(format!("connect error: {e}"));
            }
        }

        let sample = PingSample {
            seq,
            ip_addr: setting.ip_addr,
            hostname: setting.hostname.clone(),
            port: Some(port),
            rtt_ms,
            probe_status: status,
            protocol: PingProtocol::Tcp,
        };
        if sample.rtt_ms.is_some() && matches!(sample.probe_status.kind, ProbeStatusKind::Done) {
            received += 1;
        }

        let transmitted = seq;
        let percent = (seq as f32) * 100.0 / (setting.count as f32);
        let _ = app.emit(
            "ping:progress",
            PingProgressPayload {
                run_id: run_id.to_string(),
                sample: sample.clone(),
                transmitted,
                received,
                percent,
            },
        );

        samples.push(sample);

        if seq != setting.count {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(setting.send_rate_ms)) => {}
                _ = token.cancelled() => {
                    let _ = app.emit(
                        "ping:cancelled",
                        PingCancelledPayload {
                            run_id: run_id.to_string(),
                        },
                    );
                    return Err(anyhow::anyhow!("cancelled"));
                }
            }
        }
    }

    // Summarize samples
    let (min, avg, max) = summarize_rtts(&rtts_ok);
    let stat = PingStat {
        ip_addr: setting.ip_addr,
        hostname: setting.hostname.clone(),
        port: Some(port),
        protocol: PingProtocol::Tcp,
        samples: samples.clone(),
        transmitted_count: samples.len(),
        received_count: samples
            .iter()
            .filter(|s| s.rtt_ms.is_some() && s.probe_status.kind == ProbeStatusKind::Done)
            .count(),
        min,
        avg,
        max,
    };

    // Send done event
    let _ = app.emit(
        "ping:done",
        PingDonePayload {
            run_id: run_id.to_string(),
            stat: stat.clone(),
        },
    );

    Ok(stat)
}
