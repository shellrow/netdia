use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crate::model::speedtest::{
    SpeedtestDirection, SpeedtestDonePayload, SpeedtestResult, SpeedtestSetting, SpeedtestType,
    SpeedtestUpdatePayload,
};
use crate::operation::RunEmitter;
use anyhow::{bail, ensure, Context, Result};
use bytes::Bytes;
use futures_util::{stream::FuturesUnordered, StreamExt};
use rand::RngCore;
use reqwest::{header, Client, Response, StatusCode};
use serde::Deserialize;

const FOCTAL_SPEEDTEST_BASE_URL: &str = "https://speed.foctal.com";
pub(crate) const MAX_DURATION: Duration = Duration::from_secs(30);
const TOKEN_TIMEOUT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(250);
const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Deserialize)]
struct TokenResp {
    token: String,
    expires_in: i64,
}

struct SpeedtestAuth(String);

fn mbps(bytes: u64, secs: f64) -> f64 {
    if secs <= 0.0 {
        0.0
    } else {
        (bytes as f64 * 8.0) / secs / 1_000_000.0
    }
}

fn check_status(response: &Response, operation: &str) -> Result<()> {
    match response.status() {
        StatusCode::OK => Ok(()),
        StatusCode::TOO_MANY_REQUESTS => {
            let seconds = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(60);
            bail!("{operation}: rate limited; retry after {seconds} seconds")
        }
        StatusCode::UNAUTHORIZED => {
            bail!("{operation}: authorization expired or was rejected; start a new test")
        }
        StatusCode::PAYLOAD_TOO_LARGE => {
            bail!("{operation}: the server rejected this upload size; select a smaller size")
        }
        status => bail!("{operation}: HTTP {status}"),
    }
}

async fn get_token(client: &Client, base_url: &str) -> Result<SpeedtestAuth> {
    let response = client
        .get(format!("{base_url}/token"))
        .timeout(TOKEN_TIMEOUT)
        .send()
        .await
        .context("GET /token")?;
    check_status(&response, "Token request")?;
    let token: TokenResp = response.json().await.context("parse token JSON")?;
    ensure!(
        !token.token.is_empty() && token.expires_in >= 5,
        "Server returned an empty or nearly expired token"
    );
    Ok(SpeedtestAuth(token.token))
}

pub(crate) fn size_label_for_bytes(target_bytes: u64) -> Option<&'static str> {
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

fn build_client(max_duration: Duration) -> Result<Client> {
    Client::builder()
        .timeout(max_duration.max(TOKEN_TIMEOUT))
        .redirect(reqwest::redirect::Policy::none())
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .context("build speed test client")
}

fn parallel_streams(
    direction: &SpeedtestDirection,
    test_type: SpeedtestType,
    target_bytes: u64,
) -> usize {
    match (direction, test_type, target_bytes) {
        (_, _, 0..=26_214_400) => 4,
        (SpeedtestDirection::Download, SpeedtestType::ByteStream, 26_214_401..=52_428_800) => 4,
        (_, _, 26_214_401..=52_428_800) => 3,
        (SpeedtestDirection::Download, SpeedtestType::ByteStream, _) => 3,
        (_, _, _) => 2,
    }
}

async fn receive_download(
    mut response: Response,
    target: u64,
    transferred: &AtomicU64,
) -> Result<()> {
    check_status(&response, "Download")?;
    for encoding in response.headers().get_all(header::CONTENT_ENCODING) {
        ensure!(
            encoding.to_str()?.eq_ignore_ascii_case("identity"),
            "Encoded downloads cannot be used for speed measurements"
        );
    }
    if let Some(length) = response.content_length() {
        ensure!(
            length == target,
            "Download size mismatch: expected {target} bytes, received Content-Length {length}"
        );
    }
    let mut received = 0;
    while let Some(bytes) = response.chunk().await.context("read download body")? {
        ensure!(
            bytes.len() as u64 <= target - received,
            "Download exceeded the requested byte count"
        );
        received += bytes.len() as u64;
        transferred.fetch_add(bytes.len() as u64, Ordering::Relaxed);
    }
    ensure!(
        received == target,
        "Download ended before the requested byte count ({received}/{target})"
    );
    Ok(())
}

async fn receive_upload_receipt(mut response: Response, target: u64) -> Result<()> {
    check_status(&response, "Upload")?;
    let received = response
        .headers()
        .get("x-received-bytes")
        .context("Upload response is missing X-Received-Bytes")?
        .to_str()?
        .parse::<u64>()
        .context("Invalid upload receipt")?;
    ensure!(
        received == target,
        "Upload receipt mismatch: server received {received} of {target} bytes"
    );
    ensure!(
        response
            .chunk()
            .await
            .context("read upload response")?
            .is_none(),
        "Upload response must be empty"
    );
    Ok(())
}

async fn transfer(
    client: &Client,
    url: &str,
    auth: &SpeedtestAuth,
    direction: &SpeedtestDirection,
    target: u64,
    transferred: &AtomicU64,
    upload_chunk: Bytes,
) -> Result<()> {
    match direction {
        SpeedtestDirection::Download => {
            let response = client
                .get(url)
                .bearer_auth(&auth.0)
                .header(header::ACCEPT_ENCODING, "identity")
                .header(header::CACHE_CONTROL, "no-store, no-transform")
                .send()
                .await
                .context("GET download")?;
            receive_download(response, target, transferred).await
        }
        SpeedtestDirection::Upload => {
            let body = futures_util::stream::unfold(target, move |remaining| {
                let chunk = upload_chunk.clone();
                async move {
                    if remaining == 0 {
                        return None;
                    }
                    let size = remaining.min(chunk.len() as u64) as usize;
                    Some((
                        Ok::<_, std::io::Error>(chunk.slice(..size)),
                        remaining - size as u64,
                    ))
                }
            });
            let response = client
                .post(url)
                .bearer_auth(&auth.0)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CONTENT_LENGTH, target)
                .body(reqwest::Body::wrap_stream(body))
                .send()
                .await
                .context("POST upload")?;
            receive_upload_receipt(response, target).await?;
            // Body polling only measures client buffering. Count server receipts instead.
            transferred.fetch_add(target, Ordering::Relaxed);
            Ok(())
        }
    }
}

async fn run_transfers(
    client: &Client,
    base_url: &str,
    auth: &SpeedtestAuth,
    setting: &SpeedtestSetting,
    mut update: impl FnMut(SpeedtestUpdatePayload),
) -> Result<SpeedtestDonePayload> {
    let url = match (&setting.direction, setting.test_type) {
        (SpeedtestDirection::Download, SpeedtestType::ByteStream) => {
            format!("{base_url}/__down?bytes={}", setting.target_bytes)
        }
        (SpeedtestDirection::Download, SpeedtestType::FileDownload) => {
            let label = size_label_for_bytes(setting.target_bytes)
                .context("unsupported file-download size")?;
            format!("{base_url}/__filedown/random/{label}")
        }
        (SpeedtestDirection::Upload, SpeedtestType::ByteStream) => format!("{base_url}/__up"),
        _ => bail!("upload is not available for file-download tests"),
    };
    let count = parallel_streams(&setting.direction, setting.test_type, setting.target_bytes);
    let target_bytes = setting
        .target_bytes
        .checked_mul(count as u64)
        .context("Transfer size overflow")?;
    let transferred = Arc::new(AtomicU64::new(0));
    let upload_chunk = if matches!(setting.direction, SpeedtestDirection::Upload) {
        let mut bytes = vec![0; CHUNK_SIZE];
        rand::thread_rng().fill_bytes(&mut bytes);
        Bytes::from(bytes)
    } else {
        Bytes::new()
    };
    // These futures are owned by the run: cancellation drops every request immediately.
    let mut pending = FuturesUnordered::new();
    for _ in 0..count {
        pending.push(transfer(
            client,
            &url,
            auth,
            &setting.direction,
            setting.target_bytes,
            &transferred,
            upload_chunk.clone(),
        ));
    }
    let started = Instant::now();
    let duration = Duration::from_millis(
        setting
            .max_duration_ms
            .unwrap_or(MAX_DURATION.as_millis() as u64),
    );
    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_tick = started;
    let mut last_bytes = 0;
    let mut result = SpeedtestResult::Full;
    let mut message = None;
    while !pending.is_empty() {
        tokio::select! {
            biased;
            _ = &mut deadline => {
                result = SpeedtestResult::Timeout;
                break;
            }
            response = pending.next() => {
                if let Some(Err(error)) = response {
                    result = SpeedtestResult::Error;
                    message = Some(error.to_string());
                    break;
                }
            }
            _ = ticker.tick() => {
                let now = Instant::now();
                let bytes = transferred.load(Ordering::Relaxed);
                let elapsed = now - started;
                let instant = if matches!(setting.direction, SpeedtestDirection::Download) {
                    mbps(bytes.saturating_sub(last_bytes), (now - last_tick).as_secs_f64())
                } else { 0.0 };
                update(SpeedtestUpdatePayload {
                    direction: setting.direction.clone(), phase: "running".into(),
                    elapsed_ms: elapsed.as_millis() as u64,
                    transferred_bytes: bytes, target_bytes, instant_mbps: instant,
                    avg_mbps: mbps(bytes, elapsed.as_secs_f64()),
                });
                last_bytes = bytes;
                last_tick = now;
            }
        }
    }
    drop(pending);
    let elapsed = started.elapsed();
    let bytes = transferred.load(Ordering::Relaxed);
    Ok(SpeedtestDonePayload {
        direction: setting.direction.clone(),
        result,
        elapsed_ms: elapsed.as_millis() as u64,
        transferred_bytes: bytes,
        target_bytes,
        avg_mbps: mbps(bytes, elapsed.as_secs_f64()),
        message,
    })
}

pub async fn run_speedtest(
    app: &RunEmitter<'_>,
    direction: SpeedtestDirection,
    test_type: SpeedtestType,
    target_bytes: u64,
    max_duration: Duration,
) -> Result<()> {
    let client = build_client(max_duration)?;
    let auth = get_token(&client, FOCTAL_SPEEDTEST_BASE_URL).await?;
    let setting = SpeedtestSetting {
        direction,
        test_type,
        target_bytes,
        max_duration_ms: Some(max_duration.as_millis() as u64),
    };
    let result = run_transfers(
        &client,
        FOCTAL_SPEEDTEST_BASE_URL,
        &auth,
        &setting,
        |payload| {
            let _ = app.emit("speedtest:update", payload);
        },
    )
    .await?;
    let _ = app.emit("speedtest:done", result);
    Ok(())
}

#[cfg(test)]
mod tests;
