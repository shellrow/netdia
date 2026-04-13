use anyhow::{Context, Result};
use reqwest::{header, Client};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::model::speedtest::{LatencyDonePayload, LatencyUpdatePayload};

const CLOUDFLARE_PING_URL: &str = "https://speed.cloudflare.com/__down?bytes=0";
const LEGACY_PING_URL: &str = "https://speedtest.foctal.com/ping";
const CLOUDFLARE_REFERER: &str = "https://speed.cloudflare.com/";
const TICK_WAIT: Duration = Duration::from_millis(120);
pub(crate) const DEFAULT_PING_COUNT: u32 = 7;

#[derive(serde::Deserialize)]
struct PingResp {
    #[allow(dead_code)]
    ts: i64,
    colo: Option<String>,
}

fn build_client(referer: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder().timeout(Duration::from_secs(5));
    if let Some(referer) = referer {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::REFERER, header::HeaderValue::from_str(referer)?);
        builder = builder.default_headers(headers);
    }
    builder.build().context("build reqwest client")
}

async fn measure_with_cloudflare(app: &AppHandle, samples: u32) -> Result<()> {
    let client = build_client(Some(CLOUDFLARE_REFERER))?;
    let mut rtts: Vec<f64> = Vec::with_capacity(samples as usize);
    let mut colo: Option<String> = None;

    for i in 0..samples {
        let t0 = Instant::now();
        let resp = client
            .get(CLOUDFLARE_PING_URL)
            .send()
            .await
            .context("GET Cloudflare latency probe")?;
        let elapsed = t0.elapsed().as_secs_f64() * 1000.0;

        if !resp.status().is_success() {
            anyhow::bail!("cloudflare latency http {}", resp.status());
        }

        if colo.is_none() {
            colo = resp
                .headers()
                .get("cf-meta-colo")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string());
        }

        rtts.push(elapsed);

        let _ = app.emit(
            "latency:update",
            LatencyUpdatePayload {
                phase: "running".into(),
                sample: i + 1,
                total: samples,
                rtt_ms: elapsed,
            },
        );

        tokio::time::sleep(TICK_WAIT).await;
    }

    let lat = median(rtts.clone());
    let jit = stddev(&rtts);

    let _ = app.emit(
        "latency:done",
        LatencyDonePayload {
            latency_ms: lat,
            jitter_ms: jit,
            samples: rtts,
            colo,
        },
    );

    Ok(())
}

async fn measure_with_legacy_ping(app: &AppHandle, samples: u32) -> Result<()> {
    let client = build_client(None)?;
    let mut rtts: Vec<f64> = Vec::with_capacity(samples as usize);
    let mut colo: Option<String> = None;

    for i in 0..samples {
        let t0 = Instant::now();
        let resp = client
            .get(LEGACY_PING_URL)
            .send()
            .await
            .context("GET /ping")?;
        let elapsed = t0.elapsed().as_secs_f64() * 1000.0;
        rtts.push(elapsed);

        if colo.is_none() {
            if let Ok(p) = resp.json::<PingResp>().await {
                colo = p.colo;
            }
        }

        let _ = app.emit(
            "latency:update",
            LatencyUpdatePayload {
                phase: "running".into(),
                sample: i + 1,
                total: samples,
                rtt_ms: elapsed,
            },
        );

        tokio::time::sleep(TICK_WAIT).await;
    }

    let lat = median(rtts.clone());
    let jit = stddev(&rtts);

    let _ = app.emit(
        "latency:done",
        LatencyDonePayload {
            latency_ms: lat,
            jitter_ms: jit,
            samples: rtts,
            colo,
        },
    );

    Ok(())
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn stddev(v: &[f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / v.len() as f64;
    var.sqrt()
}

pub async fn measure_latency_jitter(app: &AppHandle, samples: u32) -> Result<()> {
    if let Err(error) = measure_with_cloudflare(app, samples).await {
        tracing::warn!(
            error = %error,
            "Cloudflare latency probe failed; falling back to legacy ping endpoint"
        );
        return measure_with_legacy_ping(app, samples)
            .await
            .with_context(|| format!("cloudflare latency failed: {error}"));
    }

    Ok(())
}
