use anyhow::Result;
use reqwest::Client;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::model::ping::{
    PingCancelledPayload, PingDonePayload, PingProgressPayload, PingProtocol, PingSample,
    PingSetting, PingStat,
};
use crate::model::probe::{ProbeStatus, ProbeStatusKind};
use crate::probe::DEFAULT_USER_AGENT_CHROME;

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
    (
        Some(min),
        Some((sum / rtts_ms.len() as u128) as u64),
        Some(max),
    )
}

pub async fn http_ping(
    app: &AppHandle,
    run_id: &str,
    setting: PingSetting,
    token: CancellationToken,
) -> Result<PingStat> {
    // Build HTTP client
    let per_req_to = Duration::from_millis(setting.timeout_ms);
    let client = Client::builder()
        .user_agent(DEFAULT_USER_AGENT_CHROME)
        .danger_accept_invalid_certs(true)
        .pool_idle_timeout(Some(Duration::from_secs(5)))
        .tcp_keepalive(Some(Duration::from_secs(10)))
        .timeout(per_req_to)
        .build()?;

    // Build URL
    let host = setting
        .hostname
        .clone()
        .unwrap_or_else(|| setting.ip_addr.to_string());
    let port = setting.port.unwrap_or_else(|| {
        if host.starts_with("https://") {
            443
        } else {
            80
        }
    });

    let url = if host.starts_with("http://") || host.starts_with("https://") {
        host
    } else {
        let scheme = if port == 443 { "https" } else { "http" };
        format!("{scheme}://{host}:{port}/")
    };

    let mut samples = Vec::with_capacity(setting.count as usize);
    let mut rtts_ok: Vec<u64> = Vec::new();

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
        let mut status = ProbeStatus::new();
        let mut rtt_ms: Option<u64> = None;

        let started = Instant::now();
        // TTBF(Time to First Byte) as RTT
        let fut = client.get(&url).send();

        match tokio::time::timeout(per_req_to, fut).await {
            Err(_) => {
                status = ProbeStatus::with_timeout_message(format!(
                    "timeout (>{}ms)",
                    setting.timeout_ms
                ));
            }
            Ok(Err(e)) => {
                status = ProbeStatus::with_error_message(format!("http error: {e}"));
            }
            Ok(Ok(resp)) => {
                let rtt = started.elapsed().as_millis() as u64;
                rtt_ms = Some(rtt);
                rtts_ok.push(rtt);
                // Consume response to avoid connection leak
                drop(resp);
            }
        }

        let sample = PingSample {
            seq,
            ip_addr: setting.ip_addr,
            hostname: setting.hostname.clone(),
            port: Some(port),
            rtt_ms,
            probe_status: status,
            protocol: PingProtocol::Http,
        };

        if sample.rtt_ms.is_some() && matches!(sample.probe_status.kind, ProbeStatusKind::Done) {
            received += 1;
        }

        let transmitted = seq;
        let percent = (seq as f32) * 100.0 / (setting.count as f32);
        // Send progress event
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
        protocol: PingProtocol::Http,
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
