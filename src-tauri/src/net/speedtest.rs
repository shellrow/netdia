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
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const FOCTAL_SPEEDTEST_BASE_URL: &str = "https://speed.foctal.com";
pub(crate) const MAX_DURATION: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(250);
const CHUNK_SIZE: usize = 64 * 1024;
const WARMUP: Duration = Duration::from_millis(750);

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
    cancel: CancellationToken,
}

#[derive(Clone, Copy)]
struct StableAverage {
    elapsed: Duration,
    transferred_bytes: u64,
}

struct ProgressState {
    start: Instant,
    last_tick: Instant,
    last_bytes: u64,
    warmup_marked: bool,
    warmup_bytes: u64,
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

fn parallel_streams(
    direction: SpeedtestDirection,
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

fn total_target_bytes(per_stream_target: u64, streams: usize) -> u64 {
    per_stream_target.saturating_mul(streams as u64)
}

fn init_progress() -> ProgressState {
    let start = Instant::now();
    ProgressState {
        start,
        last_tick: start,
        last_bytes: 0,
        warmup_marked: false,
        warmup_bytes: 0,
    }
}

fn compute_stable_average(
    progress: &mut ProgressState,
    transferred: u64,
    elapsed: Duration,
) -> StableAverage {
    if !progress.warmup_marked && elapsed >= WARMUP {
        progress.warmup_marked = true;
        progress.warmup_bytes = transferred;
    }

    if progress.warmup_marked {
        StableAverage {
            elapsed: elapsed.saturating_sub(WARMUP),
            transferred_bytes: transferred.saturating_sub(progress.warmup_bytes),
        }
    } else {
        StableAverage {
            elapsed,
            transferred_bytes: transferred,
        }
    }
}

fn emit_progress_update(
    app: &AppHandle,
    direction: SpeedtestDirection,
    progress: &mut ProgressState,
    transferred: u64,
    target_bytes: u64,
) {
    let elapsed = progress.start.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;

    let now = Instant::now();
    let dt = (now - progress.last_tick).as_secs_f64().max(1e-6);
    let dbytes = transferred.saturating_sub(progress.last_bytes);
    let instant = mbps(dbytes, dt);

    let stable = compute_stable_average(progress, transferred, elapsed);
    let avg = mbps(stable.transferred_bytes, stable.elapsed.as_secs_f64());

    progress.last_tick = now;
    progress.last_bytes = transferred;

    emit_speedtest_update(
        app,
        direction,
        elapsed_ms,
        transferred,
        target_bytes,
        instant,
        avg,
    );
}

fn final_average(progress: &mut ProgressState, transferred: u64) -> (u64, f64) {
    let elapsed = progress.start.elapsed();
    let stable = compute_stable_average(progress, transferred, elapsed);
    let avg = mbps(stable.transferred_bytes, stable.elapsed.as_secs_f64());
    (elapsed.as_millis() as u64, avg)
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
    let stream_count = parallel_streams(direction.clone(), test_type, target_bytes);

    match (direction, test_type) {
        (SpeedtestDirection::Download, SpeedtestType::ByteStream) => {
            download_test(app, client, auth, target_bytes, stream_count, max_duration).await?;
        }
        (SpeedtestDirection::Download, SpeedtestType::FileDownload) => {
            file_download_test(app, client, auth, target_bytes, stream_count, max_duration).await?;
        }
        (SpeedtestDirection::Upload, SpeedtestType::ByteStream) => {
            upload_test(app, client, auth, target_bytes, stream_count, max_duration).await?;
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
    per_stream_target_bytes: u64,
    stream_count: usize,
    max_duration: Duration,
) -> Result<()> {
    let target_bytes = total_target_bytes(per_stream_target_bytes, stream_count);
    let transferred = Arc::new(AtomicU64::new(0));
    let cancel = CancellationToken::new();
    let mut join_set = JoinSet::new();

    for _ in 0..stream_count {
        let client = client.clone();
        let auth = auth.clone();
        let transferred = transferred.clone();
        let cancel = cancel.clone();

        join_set.spawn(async move {
            let url = build_download_url(per_stream_target_bytes, &auth);
            let resp = client.get(url).send().await.context("GET download")?;

            if !resp.status().is_success() {
                anyhow::bail!("download http {}", resp.status());
            }

            let mut stream = resp.bytes_stream();
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        break;
                    }
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(b)) => {
                                transferred.fetch_add(b.len() as u64, Ordering::Relaxed);
                            }
                            Some(Err(e)) => {
                                return Err(anyhow::anyhow!(e));
                            }
                            None => break,
                        }
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        });
    }

    let mut progress = init_progress();
    let mut ticker = tokio::time::interval(TICK);
    let mut result = SpeedtestResult::Full;
    let mut completed = 0usize;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let transferred_now = transferred.load(Ordering::Relaxed);
                emit_progress_update(app, SpeedtestDirection::Download, &mut progress, transferred_now, target_bytes);

                let elapsed = progress.start.elapsed();
                if elapsed >= max_duration {
                    result = SpeedtestResult::Timeout;
                    cancel.cancel();
                    join_set.abort_all();
                    break;
                }
                if transferred_now >= target_bytes {
                    result = SpeedtestResult::Full;
                    cancel.cancel();
                    break;
                }
            }
            joined = join_set.join_next(), if completed < stream_count => {
                match joined {
                    Some(Ok(Ok(()))) => {
                        completed += 1;
                        if completed == stream_count {
                            break;
                        }
                    }
                    Some(Ok(Err(err))) => {
                        cancel.cancel();
                        join_set.abort_all();
                        return Err(err);
                    }
                    Some(Err(join_err)) => {
                        if join_err.is_cancelled() {
                            break;
                        }
                        cancel.cancel();
                        join_set.abort_all();
                        return Err(anyhow::anyhow!(join_err));
                    }
                    None => break,
                }
            }
        }
    }

    cancel.cancel();
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}

    let transferred = transferred.load(Ordering::Relaxed);
    let (elapsed_ms, avg) = final_average(&mut progress, transferred);

    emit_speedtest_done(
        app,
        SpeedtestDonePayload {
            direction: SpeedtestDirection::Download,
            result,
            elapsed_ms,
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
    per_stream_target_bytes: u64,
    stream_count: usize,
    max_duration: Duration,
) -> Result<()> {
    let target_bytes = total_target_bytes(per_stream_target_bytes, stream_count);
    let transferred = Arc::new(AtomicU64::new(0));
    let cancel = CancellationToken::new();
    let mut join_set = JoinSet::new();

    for _ in 0..stream_count {
        let client = client.clone();
        let auth = auth.clone();
        let transferred = transferred.clone();
        let cancel = cancel.clone();

        join_set.spawn(async move {
            let url = build_file_download_url(per_stream_target_bytes, &auth)?;
            let resp = client.get(url).send().await.context("GET file download")?;

            if !resp.status().is_success() {
                anyhow::bail!("file download http {}", resp.status());
            }

            let mut stream = resp.bytes_stream();
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        break;
                    }
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(b)) => {
                                transferred.fetch_add(b.len() as u64, Ordering::Relaxed);
                            }
                            Some(Err(e)) => {
                                return Err(anyhow::anyhow!(e));
                            }
                            None => break,
                        }
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        });
    }

    let mut progress = init_progress();
    let mut ticker = tokio::time::interval(TICK);
    let mut result = SpeedtestResult::Full;
    let mut completed = 0usize;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let transferred_now = transferred.load(Ordering::Relaxed);
                emit_progress_update(app, SpeedtestDirection::Download, &mut progress, transferred_now, target_bytes);

                let elapsed = progress.start.elapsed();
                if elapsed >= max_duration {
                    result = SpeedtestResult::Timeout;
                    cancel.cancel();
                    join_set.abort_all();
                    break;
                }
                if transferred_now >= target_bytes {
                    result = SpeedtestResult::Full;
                    cancel.cancel();
                    break;
                }
            }
            joined = join_set.join_next(), if completed < stream_count => {
                match joined {
                    Some(Ok(Ok(()))) => {
                        completed += 1;
                        if completed == stream_count {
                            break;
                        }
                    }
                    Some(Ok(Err(err))) => {
                        cancel.cancel();
                        join_set.abort_all();
                        return Err(err);
                    }
                    Some(Err(join_err)) => {
                        if join_err.is_cancelled() {
                            break;
                        }
                        cancel.cancel();
                        join_set.abort_all();
                        return Err(anyhow::anyhow!(join_err));
                    }
                    None => break,
                }
            }
        }
    }

    cancel.cancel();
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}

    let transferred = transferred.load(Ordering::Relaxed);
    let (elapsed_ms, avg) = final_average(&mut progress, transferred);

    emit_speedtest_done(
        app,
        SpeedtestDonePayload {
            direction: SpeedtestDirection::Download,
            result,
            elapsed_ms,
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
    per_stream_target_bytes: u64,
    stream_count: usize,
    max_duration: Duration,
) -> Result<()> {
    let target_bytes = total_target_bytes(per_stream_target_bytes, stream_count);
    let sent = Arc::new(AtomicU64::new(0));
    let cancel = CancellationToken::new();
    let mut join_set = JoinSet::new();

    for _ in 0..stream_count {
        let client = client.clone();
        let auth = auth.clone();
        let sent = sent.clone();
        let cancel = cancel.clone();

        join_set.spawn(async move {
            let url = build_upload_url(&auth);
            let sent2 = sent.clone();
            let cancel2 = cancel.clone();
            let body_stream = futures_util::stream::try_unfold(
                UpState {
                    remaining: per_stream_target_bytes,
                    cancel: cancel2,
                },
                move |mut st| {
                    let sent2 = sent2.clone();
                    async move {
                        if st.remaining == 0 || st.cancel.is_cancelled() {
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

            let resp = apply_upload_auth(client.post(url), &auth)
                .body(reqwest::Body::wrap_stream(body_stream))
                .send()
                .await?;

            if !resp.status().is_success() {
                anyhow::bail!("upload http {}", resp.status());
            }

            Ok::<(), anyhow::Error>(())
        });
    }

    let mut progress = init_progress();
    let mut ticker = tokio::time::interval(TICK);
    let mut result = SpeedtestResult::Full;
    let mut completed = 0usize;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let transferred = sent.load(Ordering::Relaxed);
                emit_progress_update(app, SpeedtestDirection::Upload, &mut progress, transferred, target_bytes);

                let elapsed = progress.start.elapsed();
                if elapsed >= max_duration {
                    result = SpeedtestResult::Timeout;
                    cancel.cancel();
                    join_set.abort_all();
                    break;
                }
                if transferred >= target_bytes {
                    result = SpeedtestResult::Full;
                    cancel.cancel();
                }
            }
            joined = join_set.join_next(), if completed < stream_count => {
                match joined {
                    Some(Ok(Ok(()))) => {
                        completed += 1;
                        if completed == stream_count {
                            break;
                        }
                    }
                    Some(Ok(Err(err))) => {
                        cancel.cancel();
                        join_set.abort_all();
                        return Err(err);
                    }
                    Some(Err(join_err)) => {
                        if join_err.is_cancelled() {
                            result = SpeedtestResult::Canceled;
                            break;
                        }
                        cancel.cancel();
                        join_set.abort_all();
                        return Err(anyhow::anyhow!(join_err));
                    }
                    None => break,
                }
            }
        }
    }

    cancel.cancel();
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}

    let transferred = sent.load(Ordering::Relaxed);
    let (elapsed_ms, avg) = final_average(&mut progress, transferred);

    emit_speedtest_done(
        app,
        SpeedtestDonePayload {
            direction: SpeedtestDirection::Upload,
            result,
            elapsed_ms,
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
