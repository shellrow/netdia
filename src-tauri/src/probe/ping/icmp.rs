use anyhow::Result;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

use crate::{
    model::{
        ping::{
            PingDonePayload, PingProgressPayload, PingProtocol, PingSample, PingSetting, PingStat,
        },
        probe::{ProbeStatus, ProbeStatusKind},
    },
    probe::packet::{build_icmp_echo_bytes, parse_icmp_echo_v4, parse_icmp_echo_v6},
    socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind},
};

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

pub async fn icmp_ping(
    app: &AppHandle,
    run_id: &str,
    src_ip: IpAddr,
    setting: PingSetting,
) -> Result<PingStat> {
    let cfg = if setting.ip_addr.is_ipv4() {
        let mut c = IcmpConfig::new(IcmpKind::V4);
        c = c.with_ttl(setting.hop_limit as u32);
        c
    } else {
        let mut c = IcmpConfig::new(IcmpKind::V6);
        c = c.with_hoplimit(setting.hop_limit as u32);
        c
    };

    let socket = match AsyncIcmpSocket::new(&cfg).await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            return Err(anyhow::anyhow!("failed to create ICMP socket: {}", e));
        }
    };

    let target = SocketAddr::new(setting.ip_addr, 0);

    let echo_id: u16 = 0x1234;
    let payload = b"netd";

    let mut samples = Vec::with_capacity(setting.count as usize);
    let mut rtts_ok = Vec::<u64>::new();

    let mut received = 0u32;

    for seq in 1..=setting.count {
        let pkt = build_icmp_echo_bytes(src_ip, setting.ip_addr, echo_id, seq as u16, payload);

        let sent_at = Instant::now();
        let mut status = ProbeStatus::new();
        let mut rtt_ms = None;

        // Send ICMP Echo Request
        if let Err(e) = socket.send_to(&pkt, target).await {
            status = ProbeStatus::with_error_message(format!("send error: {e}"));
        } else {
            // Wait for response (with timeout)
            let mut buf = vec![0u8; 2048];
            let to = Duration::from_millis(setting.timeout_ms);
            match tokio::time::timeout(to, socket.recv_from(&mut buf)).await {
                Err(_) => {
                    status = ProbeStatus::with_timeout_message(format!(
                        "timeout (>{}ms)",
                        setting.timeout_ms
                    ));
                }
                Ok(Err(e)) => {
                    status = ProbeStatus::with_error_message(format!("recv error: {e}"));
                }
                Ok(Ok((n, _addr))) => {
                    let ok = match setting.ip_addr {
                        IpAddr::V4(_) => parse_icmp_echo_v4(&buf[..n]).is_some(),
                        IpAddr::V6(_) => parse_icmp_echo_v6(&buf[..n]).is_some(),
                    };
                    if ok {
                        let rtt = sent_at.elapsed().as_millis() as u64;
                        rtt_ms = Some(rtt);
                        rtts_ok.push(rtt);
                    } else {
                        status = ProbeStatus::with_error_message("unexpected reply".to_string());
                    }
                }
            }
        }

        let sample = PingSample {
            seq,
            ip_addr: setting.ip_addr,
            hostname: setting.hostname.clone(),
            port: None,
            rtt_ms,
            probe_status: status,
            protocol: PingProtocol::Icmp,
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
            tokio::time::sleep(Duration::from_millis(setting.send_rate_ms)).await;
        }
    }

    // Summarize statistics
    let transmitted = samples.len();
    let received = samples
        .iter()
        .filter(|s| s.rtt_ms.is_some() && s.probe_status.kind == ProbeStatusKind::Done)
        .count();
    let (min, avg, max) = summarize_rtts(&rtts_ok);

    let stat = PingStat {
        ip_addr: setting.ip_addr,
        hostname: setting.hostname.clone(),
        port: None,
        protocol: PingProtocol::Icmp,
        samples,
        transmitted_count: transmitted,
        received_count: received,
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
