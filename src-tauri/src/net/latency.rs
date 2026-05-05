use anyhow::{Context, Result};
use reqwest::{header, Client};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::model::speedtest::{LatencyDonePayload, LatencyUpdatePayload, SpeedtestServer};

const CLOUDFLARE_REFERER: &str = "https://speed.cloudflare.com/";
const TICK_WAIT: Duration = Duration::from_millis(120);
pub(crate) const DEFAULT_PING_COUNT: u32 = 7;

fn build_client(referer: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder().timeout(Duration::from_secs(5));
    if let Some(referer) = referer {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::REFERER, header::HeaderValue::from_str(referer)?);
        builder = builder.default_headers(headers);
    }
    builder.build().context("build reqwest client")
}

fn base_url(server: SpeedtestServer) -> &'static str {
    match server {
        SpeedtestServer::Cloudflare => "https://speed.cloudflare.com",
        SpeedtestServer::Foctal => "https://speed.foctal.com",
    }
}

fn referer(server: SpeedtestServer) -> Option<&'static str> {
    match server {
        SpeedtestServer::Cloudflare => Some(CLOUDFLARE_REFERER),
        SpeedtestServer::Foctal => None,
    }
}

fn latency_probe_url(server: SpeedtestServer) -> String {
    format!("{}/__down?bytes=0", base_url(server))
}

async fn measure_with_client<F>(
    app: &AppHandle,
    client: &Client,
    samples: u32,
    mut probe: F,
) -> Result<()>
where
    F: for<'a> FnMut(
        &'a Client,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(f64, Option<String>)>> + Send + 'a>,
    >,
{
    let mut rtts: Vec<f64> = Vec::with_capacity(samples as usize);
    let mut colo: Option<String> = None;

    for i in 0..samples {
        let (elapsed, sample_colo) = probe(client).await?;
        rtts.push(elapsed);

        if colo.is_none() {
            colo = sample_colo;
        }

        emit_latency_update(app, i + 1, samples, elapsed);

        tokio::time::sleep(TICK_WAIT).await;
    }

    emit_latency_done(app, rtts, colo);

    Ok(())
}

pub async fn measure_latency_jitter(
    app: &AppHandle,
    server: SpeedtestServer,
    samples: u32,
) -> Result<()> {
    let client = build_client(referer(server))?;
    let probe_url = latency_probe_url(server);

    measure_with_client(app, &client, samples, |client| {
        let probe_url = probe_url.clone();
        Box::pin(async move {
            let t0 = Instant::now();
            let resp = client
                .get(&probe_url)
                .send()
                .await
                .with_context(|| format!("GET latency probe: {probe_url}"))?;
            let elapsed = t0.elapsed().as_secs_f64() * 1000.0;

            if !resp.status().is_success() {
                anyhow::bail!("latency probe http {}", resp.status());
            }

            let colo = resp
                .headers()
                .get("cf-meta-colo")
                .or_else(|| resp.headers().get("cf-ray"))
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string());

            Ok((elapsed, colo))
        })
    })
    .await
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

fn emit_latency_update(app: &AppHandle, sample: u32, total: u32, rtt_ms: f64) {
    let _ = app.emit(
        "latency:update",
        LatencyUpdatePayload {
            phase: "running".into(),
            sample,
            total,
            rtt_ms,
        },
    );
}

fn emit_latency_done(app: &AppHandle, samples: Vec<f64>, colo: Option<String>) {
    let latency_ms = median(samples.clone());
    let jitter_ms = stddev(&samples);

    let _ = app.emit(
        "latency:done",
        LatencyDonePayload {
            latency_ms,
            jitter_ms,
            samples,
            colo,
        },
    );
}
