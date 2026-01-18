use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::Client;
use tauri::{AppHandle, Emitter};
use serde::Deserialize;
use crate::model::speedtest::{
    SpeedtestDirection, SpeedtestDonePayload, SpeedtestResult, SpeedtestUpdatePayload,
};

const SPEEDTEST_BASE_URL: &str = "https://speedtest.foctal.com";
pub(crate) const MAX_DURATION: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(250);
const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Deserialize)]
struct TokenResp {
    token: String,
    #[allow(dead_code)]
    expires_in: i64,
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

async fn get_token(client: &Client) -> Result<String> {
    let url = format!("{}/token", SPEEDTEST_BASE_URL);
    let resp = client.get(url).send().await.context("GET /token")?;
    if !resp.status().is_success() {
        anyhow::bail!("token http {}", resp.status());
    }
    let tr: TokenResp = resp.json().await.context("parse token json")?;
    Ok(tr.token)
}

pub async fn run_speedtest(
    app: &AppHandle,
    direction: SpeedtestDirection,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let client = Client::builder()
        .timeout(max_duration + Duration::from_secs(5))
        .build()
        .context("build reqwest client")?;

    let token = get_token(&client).await?;

    match direction {
        SpeedtestDirection::Download => {
            download_test(app, &client, &token, target_bytes, max_duration).await?;
        }
        SpeedtestDirection::Upload => {
            upload_test(app, &client, &token, target_bytes, max_duration).await?;
        }
    }

    Ok(())
}

async fn download_test(
    app: &AppHandle,
    client: &Client,
    token: &str,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let url = format!("{}/download?bytes={}", SPEEDTEST_BASE_URL, target_bytes);

    let resp = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .context("GET /download")?;

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

                let _ = app.emit("speedtest:update", SpeedtestUpdatePayload{
                    direction: SpeedtestDirection::Download,
                    phase: "running".into(),
                    elapsed_ms,
                    transferred_bytes: transferred,
                    target_bytes,
                    instant_mbps: instant,
                    avg_mbps: avg,
                });

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
                        // Stream error or aborted
                        let _ = app.emit("speedtest:done", SpeedtestDonePayload{
                            direction: SpeedtestDirection::Download,
                            result: SpeedtestResult::Error,
                            elapsed_ms: start.elapsed().as_millis() as u64,
                            transferred_bytes: transferred,
                            target_bytes,
                            avg_mbps: mbps(transferred, start.elapsed().as_secs_f64()),
                            message: Some(e.to_string()),
                        });
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

    let _ = app.emit("speedtest:done", SpeedtestDonePayload{
        direction: SpeedtestDirection::Download,
        result,
        elapsed_ms: elapsed.as_millis() as u64,
        transferred_bytes: transferred,
        target_bytes,
        avg_mbps: avg,
        message: None,
    });

    Ok(())
}

async fn upload_test(
    app: &AppHandle,
    client: &Client,
    token: &str,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let url = format!("{}/upload", SPEEDTEST_BASE_URL);

    let start = Instant::now();
    let sent = Arc::new(AtomicU64::new(0));

    let sent2 = sent.clone();
    let body_stream = futures_util::stream::try_unfold(
        UpState { remaining: target_bytes, start },
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

    let req_fut = client
        .post(url)
        .bearer_auth(token)
        .body(reqwest::Body::wrap_stream(body_stream))
        .send();

    // Run upload task and emit progress
    let mut ticker = tokio::time::interval(TICK);
    let mut last_tick = start;
    let mut last_bytes = 0u64;

    let mut req_handle = tokio::spawn(async move { req_fut.await });

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

                let _ = app.emit("speedtest:update", SpeedtestUpdatePayload{
                    direction: SpeedtestDirection::Upload,
                    phase: "running".into(),
                    elapsed_ms,
                    transferred_bytes: transferred,
                    target_bytes,
                    instant_mbps: instant,
                    avg_mbps: avg,
                });

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
                            result = SpeedtestResult::Error;
                            let _ = app.emit("speedtest:done", SpeedtestDonePayload{
                                direction: SpeedtestDirection::Upload,
                                result,
                                elapsed_ms: start.elapsed().as_millis() as u64,
                                transferred_bytes: sent.load(Ordering::Relaxed),
                                target_bytes,
                                avg_mbps: mbps(sent.load(Ordering::Relaxed), start.elapsed().as_secs_f64()),
                                message: Some(format!("upload http {}", resp.status())),
                            });
                            anyhow::bail!("upload http {}", resp.status());
                        }
                        break;
                    }
                    Ok(Err(e)) => {
                        result = SpeedtestResult::Error;
                        let _ = app.emit("speedtest:done", SpeedtestDonePayload{
                            direction: SpeedtestDirection::Upload,
                            result,
                            elapsed_ms: start.elapsed().as_millis() as u64,
                            transferred_bytes: sent.load(Ordering::Relaxed),
                            target_bytes,
                            avg_mbps: mbps(sent.load(Ordering::Relaxed), start.elapsed().as_secs_f64()),
                            message: Some(e.to_string()),
                        });
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

    let _ = app.emit("speedtest:done", SpeedtestDonePayload{
        direction: SpeedtestDirection::Upload,
        result,
        elapsed_ms: elapsed.as_millis() as u64,
        transferred_bytes: transferred,
        target_bytes,
        avg_mbps: avg,
        message: None,
    });

    Ok(())
}
