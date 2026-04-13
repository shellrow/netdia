use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crate::model::speedtest::{
    SpeedtestDirection, SpeedtestDonePayload, SpeedtestResult, SpeedtestUpdatePayload,
};
use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::{header, Client, RequestBuilder};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};

const CLOUDFLARE_SPEEDTEST_BASE_URL: &str = "https://speed.cloudflare.com";
const LEGACY_SPEEDTEST_BASE_URL: &str = "https://speedtest.foctal.com";
const CLOUDFLARE_REFERER: &str = "https://speed.cloudflare.com/";
pub(crate) const MAX_DURATION: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(250);
const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Deserialize)]
struct TokenResp {
    token: String,
    #[allow(dead_code)]
    expires_in: i64,
}

#[derive(Clone)]
enum SpeedtestAuth {
    None,
    Bearer(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpeedtestBackend {
    Cloudflare,
    Legacy,
}

/// State for upload stream
#[derive(Clone)]
struct UpState {
    remaining: u64,
    start: Instant,
}

fn mbps(bytes: u64, secs: f64) -> f64 {
    if secs <= 0.0 {
        0.0
    } else {
        (bytes as f64 * 8.0) / secs / 1_000_000.0
    }
}

impl SpeedtestBackend {
    fn base_url(self) -> &'static str {
        match self {
            SpeedtestBackend::Cloudflare => CLOUDFLARE_SPEEDTEST_BASE_URL,
            SpeedtestBackend::Legacy => LEGACY_SPEEDTEST_BASE_URL,
        }
    }

    fn referer(self) -> Option<&'static str> {
        match self {
            SpeedtestBackend::Cloudflare => Some(CLOUDFLARE_REFERER),
            SpeedtestBackend::Legacy => None,
        }
    }

    fn download_path(self) -> &'static str {
        match self {
            SpeedtestBackend::Cloudflare => "__down",
            SpeedtestBackend::Legacy => "download",
        }
    }

    fn upload_path(self) -> &'static str {
        match self {
            SpeedtestBackend::Cloudflare => "__up",
            SpeedtestBackend::Legacy => "upload",
        }
    }
}

async fn get_token(client: &Client) -> Result<String> {
    let url = format!("{}/token", SpeedtestBackend::Legacy.base_url());
    let resp = client.get(url).send().await.context("GET /token")?;
    if !resp.status().is_success() {
        anyhow::bail!("token http {}", resp.status());
    }
    let tr: TokenResp = resp.json().await.context("parse token json")?;
    Ok(tr.token)
}

fn apply_auth(builder: RequestBuilder, auth: &SpeedtestAuth) -> RequestBuilder {
    match auth {
        SpeedtestAuth::None => builder,
        SpeedtestAuth::Bearer(token) => builder.bearer_auth(token),
    }
}

fn build_client(max_duration: Duration, referer: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder().timeout(max_duration + Duration::from_secs(5));
    if let Some(referer) = referer {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::REFERER, header::HeaderValue::from_str(referer)?);
        builder = builder.default_headers(headers);
    }
    builder.build().context("build reqwest client")
}

async fn run_speedtest_with_backend(
    app: &AppHandle,
    client: &Client,
    backend: SpeedtestBackend,
    auth: &SpeedtestAuth,
    direction: SpeedtestDirection,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    match direction {
        SpeedtestDirection::Download => {
            download_test(app, client, backend, auth, target_bytes, max_duration).await?;
        }
        SpeedtestDirection::Upload => {
            upload_test(app, client, backend, auth, target_bytes, max_duration).await?;
        }
    }

    Ok(())
}

pub async fn run_speedtest(
    app: &AppHandle,
    direction: SpeedtestDirection,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let cloudflare_client = build_client(max_duration, SpeedtestBackend::Cloudflare.referer())?;
    let cloudflare_attempt = run_speedtest_with_backend(
        app,
        &cloudflare_client,
        SpeedtestBackend::Cloudflare,
        &SpeedtestAuth::None,
        direction.clone(),
        target_bytes,
        max_duration,
    )
    .await;

    if let Err(cloudflare_error) = cloudflare_attempt {
        tracing::warn!(
            error = %cloudflare_error,
            "Cloudflare speed test failed; falling back to legacy speed test backend"
        );

        let legacy_client = build_client(max_duration, SpeedtestBackend::Legacy.referer())?;
        let legacy_token = get_token(&legacy_client).await?;

        return run_speedtest_with_backend(
            app,
            &legacy_client,
            SpeedtestBackend::Legacy,
            &SpeedtestAuth::Bearer(legacy_token),
            direction,
            target_bytes,
            max_duration,
        )
        .await
        .with_context(|| format!("cloudflare backend failed: {cloudflare_error}"));
    }

    Ok(())
}

async fn download_test(
    app: &AppHandle,
    client: &Client,
    backend: SpeedtestBackend,
    auth: &SpeedtestAuth,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let url = format!(
        "{}/{}?bytes={target_bytes}",
        backend.base_url(),
        backend.download_path()
    );

    let resp = apply_auth(client.get(url), auth)
        .send()
        .await
        .context("GET download")?;

    if !resp.status().is_success() {
        anyhow::bail!("download http {}", resp.status());
    }

    let start = Instant::now();
    let mut last_tick = start;
    let mut last_bytes: u64 = 0;
    let mut transferred: u64 = 0;

    let mut stream = resp.bytes_stream();
    let mut ticker = tokio::time::interval(TICK);

    let mut result = SpeedtestResult::Full;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let elapsed = start.elapsed();
                let elapsed_ms = elapsed.as_millis() as u64;

                let dt = (Instant::now() - last_tick).as_secs_f64().max(1e-6);
                let dbytes = transferred.saturating_sub(last_bytes);
                let instant = mbps(dbytes, dt);
                let avg = mbps(transferred, elapsed.as_secs_f64());

                last_tick = Instant::now();
                last_bytes = transferred;

                emit_speedtest_update(
                    app,
                    SpeedtestDirection::Download,
                    elapsed_ms,
                    transferred,
                    target_bytes,
                    instant,
                    avg,
                );

                if elapsed >= max_duration {
                    result = SpeedtestResult::Timeout;
                    break;
                }
                if transferred >= target_bytes {
                    result = SpeedtestResult::Full;
                    break;
                }
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(b)) => {
                        transferred += b.len() as u64;
                        if transferred >= target_bytes {
                            result = SpeedtestResult::Full;
                            break;
                        }
                        if start.elapsed() >= max_duration {
                            result = SpeedtestResult::Timeout;
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!(e));
                    }
                    None => {
                        // End of stream
                        break;
                    }
                }
            }
        }
    }

    let elapsed = start.elapsed();
    let avg = mbps(transferred, elapsed.as_secs_f64());

    emit_speedtest_done(
        app,
        SpeedtestDonePayload {
            direction: SpeedtestDirection::Download,
            result,
            elapsed_ms: elapsed.as_millis() as u64,
            transferred_bytes: transferred,
            target_bytes,
            avg_mbps: avg,
            message: None,
        },
    );

    Ok(())
}

async fn upload_test(
    app: &AppHandle,
    client: &Client,
    backend: SpeedtestBackend,
    auth: &SpeedtestAuth,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let url = format!("{}/{}", backend.base_url(), backend.upload_path());

    let start = Instant::now();
    let sent = Arc::new(AtomicU64::new(0));

    let sent2 = sent.clone();
    let body_stream = futures_util::stream::try_unfold(
        UpState {
            remaining: target_bytes,
            start,
        },
        move |mut st| {
            let sent2 = sent2.clone();
            async move {
                if st.remaining == 0 {
                    return Ok::<Option<(Bytes, UpState)>, anyhow::Error>(None);
                }
                if st.start.elapsed() >= max_duration {
                    return Ok::<Option<(Bytes, UpState)>, anyhow::Error>(None);
                }

                let take = (st.remaining.min(CHUNK_SIZE as u64)) as usize;
                let buf = Bytes::from(vec![0u8; take]);

                st.remaining -= take as u64;
                sent2.fetch_add(take as u64, Ordering::Relaxed);

                Ok(Some((buf, st)))
            }
        },
    );

    let req_fut = apply_auth(client.post(url), auth)
        .body(reqwest::Body::wrap_stream(body_stream))
        .send();

    // Run upload task and emit progress
    let mut ticker = tokio::time::interval(TICK);
    let mut last_tick = start;
    let mut last_bytes = 0u64;

    let mut req_handle = tokio::spawn(req_fut);

    let mut result = SpeedtestResult::Full;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let elapsed = start.elapsed();
                let elapsed_ms = elapsed.as_millis() as u64;
                let transferred = sent.load(Ordering::Relaxed);

                let dt = (Instant::now() - last_tick).as_secs_f64().max(1e-6);
                let dbytes = transferred.saturating_sub(last_bytes);
                let instant = mbps(dbytes, dt);
                let avg = mbps(transferred, elapsed.as_secs_f64());

                last_tick = Instant::now();
                last_bytes = transferred;

                emit_speedtest_update(
                    app,
                    SpeedtestDirection::Upload,
                    elapsed_ms,
                    transferred,
                    target_bytes,
                    instant,
                    avg,
                );

                if elapsed >= max_duration {
                    result = SpeedtestResult::Timeout;
                    // abort request task (drops request/body)
                    req_handle.abort();
                    break;
                }
                if transferred >= target_bytes {
                    result = SpeedtestResult::Full;
                    // Wait for response
                }
            }
            r = &mut req_handle => {
                // Request finished
                match r {
                    Ok(Ok(resp)) => {
                        if !resp.status().is_success() {
                            anyhow::bail!("upload http {}", resp.status());
                        }
                        break;
                    }
                    Ok(Err(e)) => {
                        return Err(anyhow::anyhow!(e));
                    }
                    Err(_join_err) => {
                        // aborted
                        result = SpeedtestResult::Canceled;
                        break;
                    }
                }
            }
        }
    }

    let elapsed = start.elapsed();
    let transferred = sent.load(Ordering::Relaxed);
    let avg = mbps(transferred, elapsed.as_secs_f64());

    emit_speedtest_done(
        app,
        SpeedtestDonePayload {
            direction: SpeedtestDirection::Upload,
            result,
            elapsed_ms: elapsed.as_millis() as u64,
            transferred_bytes: transferred,
            target_bytes,
            avg_mbps: avg,
            message: None,
        },
    );

    Ok(())
}

fn emit_speedtest_update(
    app: &AppHandle,
    direction: SpeedtestDirection,
    elapsed_ms: u64,
    transferred_bytes: u64,
    target_bytes: u64,
    instant_mbps: f64,
    avg_mbps: f64,
) {
    let _ = app.emit(
        "speedtest:update",
        SpeedtestUpdatePayload {
            direction,
            phase: "running".into(),
            elapsed_ms,
            transferred_bytes,
            target_bytes,
            instant_mbps,
            avg_mbps,
        },
    );
}

fn emit_speedtest_done(app: &AppHandle, payload: SpeedtestDonePayload) {
    let _ = app.emit("speedtest:done", payload);
}
