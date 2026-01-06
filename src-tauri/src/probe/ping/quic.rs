use anyhow::Result;
use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

use crate::model::ping::{
    PingDonePayload, PingProgressPayload, PingProtocol, PingSample, PingSetting, PingStat,
};
use crate::model::probe::{ProbeStatus, ProbeStatusKind};
use crate::socket::quic::{AsyncQuicSocket, QuicConfig};
use crate::socket::SocketFamily;

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

pub async fn quic_ping(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: PingSetting,
) -> Result<PingStat> {
    let family = match setting.ip_addr {
        IpAddr::V4(_) => SocketFamily::IPV4,
        IpAddr::V6(_) => SocketFamily::IPV6,
    };
    let cfg = QuicConfig {
        skip_verify: true,
        alpn: vec![b"h3".to_vec(), b"hq-29".to_vec(), b"hq-interop".to_vec()],
        family,
    };

    let port = setting.port.unwrap_or(443);
    let target = SocketAddr::new(setting.ip_addr, port);

    let server_name = setting
        .hostname
        .clone()
        .unwrap_or_else(|| "localhost".to_string());

    let mut samples = Vec::with_capacity(setting.count as usize);
    let mut rtts_ok = Vec::<u64>::new();

    let mut received = 0u32;

    for seq in 1..=setting.count {
        let sock = AsyncQuicSocket::from_config(&cfg)?;
        let begin = Instant::now();
        let mut status = ProbeStatus::new();
        let mut rtt_ms: Option<u64> = None;

        // Connect with timeout
        let to = Duration::from_millis(setting.timeout_ms);
        match sock.connect_timeout(&target, &server_name, to).await {
            Ok(conn) => {
                let rtt = begin.elapsed().as_millis() as u64;
                rtt_ms = Some(rtt);
                rtts_ok.push(rtt);
                // Close connection
                conn.close(0u32.into(), b"bye");
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.to_lowercase().contains("elapsed") || msg.to_lowercase().contains("timeout")
                {
                    status = ProbeStatus::with_timeout_message(format!(
                        "timeout (>{}ms)",
                        setting.timeout_ms
                    ));
                } else {
                    status = ProbeStatus::with_error_message(format!("connect error: {msg}"));
                }
            }
        }

        let sample = PingSample {
            seq,
            ip_addr: setting.ip_addr,
            hostname: setting.hostname.clone(),
            port: Some(port),
            rtt_ms,
            probe_status: status,
            protocol: PingProtocol::Quic,
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

    // Summarize samples
    let transmitted = samples.len();
    let received = samples
        .iter()
        .filter(|s| s.rtt_ms.is_some() && s.probe_status.kind == ProbeStatusKind::Done)
        .count();
    let (min, avg, max) = summarize_rtts(&rtts_ok);

    let stat = PingStat {
        ip_addr: setting.ip_addr,
        hostname: setting.hostname.clone(),
        port: Some(port),
        protocol: PingProtocol::Quic,
        samples: samples.clone(),
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
