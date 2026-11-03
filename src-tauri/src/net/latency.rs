use crate::operation::RunEmitter;
use anyhow::{Context, Result};
use reqwest::{header, Client, StatusCode};
use std::time::{Duration, Instant};

use crate::model::speedtest::{LatencyDonePayload, LatencyUpdatePayload};

const FOCTAL_SPEEDTEST_BASE_URL: &str = "https://speed.foctal.com";
const TICK_WAIT: Duration = Duration::from_millis(120);
pub(crate) const DEFAULT_PING_COUNT: u32 = 7;

fn build_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("build reqwest client")
}

fn latency_probe_url() -> String {
    format!("{FOCTAL_SPEEDTEST_BASE_URL}/__down?bytes=0")
}

async fn measure_with_client<F>(
    app: &RunEmitter<'_>,
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

    anyhow::ensure!(samples > 0, "At least one latency sample is required");
    // Establish the connection before measuring steady-state HTTP round trips.
    probe(client).await?;

    for i in 0..samples {
        let (elapsed, sample_colo) = probe(client).await?;
        rtts.push(elapsed);

        if colo.is_none() {
            colo = sample_colo;
        }

        emit_latency_update(app, i + 1, samples, elapsed);

        if i + 1 < samples {
            tokio::time::sleep(TICK_WAIT).await;
        }
    }

    emit_latency_done(app, rtts, colo);

    Ok(())
}

pub async fn measure_latency_jitter(app: &RunEmitter<'_>, samples: u32) -> Result<()> {
    let client = build_client()?;
    let probe_url = latency_probe_url();

    measure_with_client(app, &client, samples, |client| {
        let probe_url = probe_url.clone();
        Box::pin(async move { latency_sample(client, &probe_url).await })
    })
    .await
}

async fn latency_sample(client: &Client, url: &str) -> Result<(f64, Option<String>)> {
    let started = Instant::now();
    let mut response = client
        .get(url)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .context("GET latency probe")?;
    anyhow::ensure!(
        response.status() == StatusCode::OK,
        "latency probe HTTP {}",
        response.status()
    );
    let colo = response
        .headers()
        .get("cf-meta-colo")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            response
                .headers()
                .get("cf-ray")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit_once('-'))
                .map(|(_, colo)| colo.to_owned())
        });
    while let Some(chunk) = response.chunk().await.context("read latency response")? {
        anyhow::ensure!(chunk.is_empty(), "Latency probe returned a nonempty body");
    }
    Ok((started.elapsed().as_secs_f64() * 1000.0, colo))
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
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

fn jitter(samples: &[f64]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .sum::<f64>()
        / (samples.len() - 1) as f64
}

fn emit_latency_update(app: &RunEmitter<'_>, sample: u32, total: u32, rtt_ms: f64) {
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

fn emit_latency_done(app: &RunEmitter<'_>, samples: Vec<f64>, colo: Option<String>) {
    let latency_ms = median(samples.clone());
    let jitter_ms = jitter(&samples);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_statistics_use_median_and_consecutive_differences() {
        assert_eq!(median(vec![10.0, 100.0, 20.0]), 20.0);
        assert_eq!(median(vec![10.0, 20.0]), 15.0);
        assert_eq!(jitter(&[10.0, 100.0, 20.0]), 85.0);
        assert_eq!(jitter(&[10.0]), 0.0);
    }

    #[tokio::test]
    async fn latency_waits_for_response_completion_and_extracts_edge_code() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nCF-Ray: example-NRT\r\n\r\n",
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(60)).await;
            socket.write_all(b"0\r\n\r\n").await.unwrap();
        });
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let (elapsed, colo) = latency_sample(&client, &format!("http://{address}"))
            .await
            .unwrap();
        assert!(elapsed >= 60.0);
        assert_eq!(colo.as_deref(), Some("NRT"));
        server.await.unwrap();
    }
}
