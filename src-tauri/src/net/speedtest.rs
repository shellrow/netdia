use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crate::model::speedtest::{
    SpeedtestDirection, SpeedtestDonePayload, SpeedtestResult, SpeedtestType,
    SpeedtestUpdatePayload,
};
use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};

const FOCTAL_SPEEDTEST_BASE_URL: &str = "https://speed.foctal.com";
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
struct SpeedtestAuth(String);

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
    let url = format!("{}/token", FOCTAL_SPEEDTEST_BASE_URL);
    let resp = client.get(url).send().await.context("GET /token")?;
    if !resp.status().is_success() {
        anyhow::bail!("token http {}", resp.status());
    }
    let tr: TokenResp = resp.json().await.context("parse token json")?;
    Ok(tr.token)
}

fn apply_upload_auth(builder: RequestBuilder, auth: &SpeedtestAuth) -> RequestBuilder {
    builder.bearer_auth(&auth.0)
}

fn build_download_url(target_bytes: u64, auth: &SpeedtestAuth) -> String {
    let mut url = format!("{FOCTAL_SPEEDTEST_BASE_URL}/__down?bytes={target_bytes}");
    url.push_str("&token=");
    url.push_str(&auth.0);
    url
}

fn size_label_for_bytes(target_bytes: u64) -> Option<&'static str> {
    match target_bytes {
        102_400 => Some("100kb"),
        1_048_576 => Some("1mb"),
        10_485_760 => Some("10mb"),
        26_214_400 => Some("25mb"),
        52_428_800 => Some("50mb"),
        104_857_600 => Some("100mb"),
        _ => None,
    }
}

fn build_file_download_url(target_bytes: u64, auth: &SpeedtestAuth) -> Result<String> {
    let size = size_label_for_bytes(target_bytes).context("unsupported file-download size")?;

    let mut url = format!("{FOCTAL_SPEEDTEST_BASE_URL}/__filedown/random/{size}");
    url.push_str("?token=");
    url.push_str(&auth.0);
    Ok(url)
}

fn build_upload_url(auth: &SpeedtestAuth) -> String {
    let mut url = format!("{FOCTAL_SPEEDTEST_BASE_URL}/__up");
    url.push_str("?token=");
    url.push_str(&auth.0);
    url
}

fn build_client(max_duration: Duration) -> Result<Client> {
    Client::builder()
        .timeout(max_duration + Duration::from_secs(5))
        .build()
        .context("build reqwest client")
}

async fn run_speedtest_with_server(
    app: &AppHandle,
    client: &Client,
    auth: &SpeedtestAuth,
    direction: SpeedtestDirection,
    test_type: SpeedtestType,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    match (direction, test_type) {
        (SpeedtestDirection::Download, SpeedtestType::ByteStream) => {
            download_test(app, client, auth, target_bytes, max_duration).await?;
        }
        (SpeedtestDirection::Download, SpeedtestType::FileDownload) => {
            file_download_test(app, client, auth, target_bytes, max_duration).await?;
        }
        (SpeedtestDirection::Upload, SpeedtestType::ByteStream) => {
            upload_test(app, client, auth, target_bytes, max_duration).await?;
        }
        (SpeedtestDirection::Upload, SpeedtestType::FileDownload) => {
            anyhow::bail!("upload is not available for file-download tests");
        }
    }

    Ok(())
}

pub async fn run_speedtest(
    app: &AppHandle,
    direction: SpeedtestDirection,
    test_type: SpeedtestType,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let client = build_client(max_duration)?;
    let auth = SpeedtestAuth(get_token(&client).await?);

    run_speedtest_with_server(
        app,
        &client,
        &auth,
        direction,
        test_type,
        target_bytes,
        max_duration,
    )
    .await
}

async fn download_test(
    app: &AppHandle,
    client: &Client,
    auth: &SpeedtestAuth,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let url = build_download_url(target_bytes, auth);
    let resp = client.get(url).send().await.context("GET download")?;

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

async fn file_download_test(
    app: &AppHandle,
    client: &Client,
    auth: &SpeedtestAuth,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let url = build_file_download_url(target_bytes, auth)?;
    let resp = client.get(url).send().await.context("GET file download")?;

    if !resp.status().is_success() {
        anyhow::bail!("file download http {}", resp.status());
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
    auth: &SpeedtestAuth,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let url = build_upload_url(auth);

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

    let req_fut = apply_upload_auth(client.post(url), auth)
        .body(reqwest::Body::wrap_stream(body_stream))
        .send();

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
                    req_handle.abort();
                    break;
                }
                if transferred >= target_bytes {
                    result = SpeedtestResult::Full;
                }
            }
            r = &mut req_handle => {
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
