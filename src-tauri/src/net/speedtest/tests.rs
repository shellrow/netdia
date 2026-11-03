use super::*;
use std::sync::Mutex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    closed: Arc<AtomicU64>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn server(response: Option<&str>, delay: Duration) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let closed = Arc::new(AtomicU64::new(0));
    let seen = requests.clone();
    let peers = closed.clone();
    let response = response.map(str::to_owned);
    let task = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (mut socket, _) = accepted.unwrap();
                    let seen = seen.clone();
                    let peers = peers.clone();
                    let response = response.clone();
                    tasks.spawn(async move {
                        let mut bytes = Vec::new();
                        let mut buf = [0; 8192];
                        let header_end = loop {
                            let n = socket.read(&mut buf).await.unwrap();
                            if n == 0 { return; }
                            bytes.extend_from_slice(&buf[..n]);
                            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") { break end + 4; }
                            assert!(bytes.len() < 16384);
                        };
                        let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                        let length = headers.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length: ").map(|value| value.parse::<usize>().unwrap())).unwrap_or(0);
                        let mut body = bytes.len() - header_end;
                        while body < length {
                            let n = socket.read(&mut buf).await.unwrap();
                            if n == 0 { return; }
                            body += n;
                        }
                        assert_eq!(body, length);
                        seen.lock().unwrap().push(headers);
                        tokio::time::sleep(delay).await;
                        if let Some(response) = response {
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else if socket.read(&mut buf).await.unwrap_or(0) == 0 {
                            peers.fetch_add(1, Ordering::Relaxed);
                        }
                    });
                }
                _ = tasks.join_next(), if !tasks.is_empty() => {}
            }
        }
    });
    Server {
        url,
        requests,
        closed,
        task,
    }
}
fn client() -> Client {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
}
fn setting(direction: SpeedtestDirection) -> SpeedtestSetting {
    SpeedtestSetting {
        direction,
        test_type: SpeedtestType::ByteStream,
        target_bytes: 4,
        max_duration_ms: Some(2000),
    }
}
async fn wait_for(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn downloads_require_exact_identity_bodies() {
    for (headers, body, succeeds) in [
        ("Content-Length: 4\r\n", "abcd", true),
        ("Content-Length: 3\r\n", "abc", false),
        ("Content-Length: 4\r\n", "abc", false),
        ("Content-Length: 5\r\n", "abcde", false),
        (
            "Content-Length: 4\r\nContent-Encoding: gzip\r\n",
            "abcd",
            false,
        ),
        (
            "Transfer-Encoding: chunked\r\n",
            "3\r\nabc\r\n0\r\n\r\n",
            false,
        ),
        (
            "Transfer-Encoding: chunked\r\n",
            "5\r\nabcde\r\n0\r\n\r\n",
            false,
        ),
        (
            "Transfer-Encoding: chunked\r\n",
            "4\r\nabcd\r\n0\r\n\r\n",
            true,
        ),
        (
            "Transfer-Encoding: chunked\r\n",
            "4\r\nabcd\r\n1\r\ne\r\n0\r\n\r\n",
            false,
        ),
    ] {
        let response = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{headers}\r\n{body}");
        let server = server(Some(&response), Duration::ZERO).await;
        let result = run_transfers(
            &client(),
            &server.url,
            &SpeedtestAuth("test-token".into()),
            &setting(SpeedtestDirection::Download),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            result.result == SpeedtestResult::Full,
            succeeds,
            "{headers}{body}"
        );
        if succeeds {
            assert_eq!(result.transferred_bytes, 16);
        }
        for request in server.requests.lock().unwrap().iter() {
            assert!(request.starts_with("GET /__down?bytes=4 HTTP/1.1"));
            assert!(!request.lines().next().unwrap().contains("test-token"));
            assert!(request.contains("authorization: Bearer test-token"));
            assert!(request.contains("accept-encoding: identity"));
        }
    }
}

#[tokio::test]
async fn uploads_count_only_valid_receipts() {
    for (status, receipt, succeeds) in [
        ("200 OK", "X-Received-Bytes: 4\r\n", true),
        ("200 OK", "", false),
        ("200 OK", "X-Received-Bytes: 3\r\n", false),
        ("200 OK", "X-Received-Bytes: 5\r\n", false),
        ("200 OK", "X-Received-Bytes: invalid\r\n", false),
        ("413 Payload Too Large", "", false),
    ] {
        let response =
            format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n{receipt}\r\n");
        let server = server(Some(&response), Duration::ZERO).await;
        let result = run_transfers(
            &client(),
            &server.url,
            &SpeedtestAuth("test-token".into()),
            &setting(SpeedtestDirection::Upload),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.result == SpeedtestResult::Full, succeeds);
        assert_eq!(result.transferred_bytes, if succeeds { 16 } else { 0 });
        for request in server.requests.lock().unwrap().iter() {
            assert!(request.starts_with("POST /__up HTTP/1.1"));
            assert!(request.contains("content-length: 4"));
            assert!(request.contains("authorization: Bearer test-token"));
        }
    }
}

#[tokio::test]
async fn upload_average_includes_time_waiting_for_acknowledgements() {
    let server = server(
        Some("HTTP/1.1 200 OK\r\nX-Received-Bytes: 4\r\nContent-Length: 0\r\n\r\n"),
        Duration::from_millis(900),
    )
    .await;
    let mut updates = Vec::new();
    let result = run_transfers(
        &client(),
        &server.url,
        &SpeedtestAuth("test".into()),
        &setting(SpeedtestDirection::Upload),
        |update| updates.push(update),
    )
    .await
    .unwrap();
    assert_eq!(result.result, SpeedtestResult::Full);
    assert_eq!(result.transferred_bytes, 16);
    assert!(result.elapsed_ms >= 900);
    assert!(result.avg_mbps > 0.0 && result.avg_mbps <= mbps(16, 0.9));
    assert!(updates
        .iter()
        .filter(|update| update.elapsed_ms < 900)
        .all(|update| update.transferred_bytes == 0));
    assert!(updates.iter().all(|update| update.instant_mbps == 0.0));
}

#[tokio::test]
async fn timeout_does_not_count_unacknowledged_uploads() {
    let server = server(None, Duration::ZERO).await;
    let mut setting = setting(SpeedtestDirection::Upload);
    setting.max_duration_ms = Some(80);
    let result = run_transfers(
        &client(),
        &server.url,
        &SpeedtestAuth("test".into()),
        &setting,
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(result.result, SpeedtestResult::Timeout);
    assert_eq!(result.transferred_bytes, 0);
    assert_eq!(result.avg_mbps, 0.0);
    wait_for(|| server.closed.load(Ordering::Relaxed) == 4).await;
}

#[tokio::test]
async fn cancellation_closes_all_pending_requests() {
    let server = server(None, Duration::ZERO).await;
    let client = client();
    let setting = setting(SpeedtestDirection::Download);
    let auth = SpeedtestAuth("test".into());
    let mut run = Box::pin(run_transfers(&client, &server.url, &auth, &setting, |_| {}));
    tokio::select! {
        _ = &mut run => panic!("run finished before cancellation"),
        _ = wait_for(|| server.requests.lock().unwrap().len() == 4) => {}
    }
    drop(run);
    wait_for(|| server.closed.load(Ordering::Relaxed) == 4).await;
}

#[tokio::test]
async fn tokens_respect_retry_after_and_expiry() {
    let server = server(
        Some("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 42\r\nContent-Length: 0\r\n\r\n"),
        Duration::ZERO,
    )
    .await;
    let error = get_token(&client(), &server.url)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("42 seconds"));
    assert_eq!(server.requests.lock().unwrap().len(), 1);
    for body in [
        r#"{"token":"test","expires_in":0}"#,
        r#"{"token":"","expires_in":60}"#,
    ] {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let fixture = self::server(Some(&response), Duration::ZERO).await;
        assert!(get_token(&client(), &fixture.url).await.is_err());
    }
}

#[tokio::test]
async fn file_downloads_use_existing_binary_profiles() {
    for (bytes, label) in [
        (102400, "100kb"),
        (1048576, "1mb"),
        (10485760, "10mb"),
        (26214400, "25mb"),
        (52428800, "50mb"),
        (104857600, "100mb"),
    ] {
        assert_eq!(size_label_for_bytes(bytes), Some(label));
    }
    let body = "x".repeat(102400);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let server = server(Some(&response), Duration::ZERO).await;
    let mut setting = setting(SpeedtestDirection::Download);
    setting.test_type = SpeedtestType::FileDownload;
    setting.target_bytes = 102400;
    let result = run_transfers(
        &client(),
        &server.url,
        &SpeedtestAuth("test".into()),
        &setting,
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(result.result, SpeedtestResult::Full);
    assert_eq!(result.transferred_bytes, 409600);
    assert!(server
        .requests
        .lock()
        .unwrap()
        .iter()
        .all(|request| request.starts_with("GET /__filedown/random/100kb HTTP/1.1")));
}
