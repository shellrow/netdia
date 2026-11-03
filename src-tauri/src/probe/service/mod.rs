use anyhow::{bail, Result};
use futures::stream::{self, StreamExt};
use probe::{PortProbe, PortProbeResult, ProbeContext, ServiceProbe};
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    net::TcpStream,
    time::{timeout, Instant},
};

use crate::model::endpoint::Endpoint;

pub mod db;
pub mod models;
mod payload;
mod probe;

/// Configuration for service probing
#[derive(Clone, Debug)]
pub struct ServiceProbeConfig {
    pub timeout: Duration,
    pub max_concurrency: usize,
    pub max_read_size: usize,
    pub sni: bool,
    pub skip_cert_verify: bool,
}

/// Result of service detection on multiple endpoints
pub struct ServiceDetectionResult {
    pub results: Vec<PortProbeResult>,
    #[allow(dead_code)]
    pub scan_time: Duration,
}

/// Service detector that runs probes against endpoints
pub struct ServiceDetector {
    pub config: ServiceProbeConfig,
}

impl ServiceDetector {
    /// Create a new ServiceDetector with the given configuration
    pub fn new(config: ServiceProbeConfig) -> Self {
        ServiceDetector { config }
    }
    /// Detect services on the given endpoint using configured probes
    pub async fn detect_services(
        config: ServiceProbeConfig,
        endpoint: Endpoint,
    ) -> Result<Vec<PortProbeResult>> {
        let port_probe_db = db::service::port_probe_db();
        let service_probe_db = db::service::service_probe_db();
        let concurrency = config.max_concurrency.max(1);
        let ports = endpoint.ports;
        let batches = stream::iter(ports)
            .map(move |port| {
                let hostname = endpoint.hostname.clone();
                let ip = endpoint.ip;
                let config = config.clone();
                async move {
                    // Perform service detection for each endpoint
                    let mut results: Vec<Result<PortProbeResult>> = Vec::new();
                    if let Some(probes) = port_probe_db.get(&port) {
                        for probe in probes {
                            let probe_payload = match service_probe_db.get(probe) {
                                Some(payload) => payload,
                                None => {
                                    results.push(Err(anyhow::anyhow!(
                                        "No payload for probe {:?}",
                                        probe
                                    )));
                                    continue;
                                }
                            };
                            let port_probe: PortProbe = PortProbe {
                                probe_id: probe.clone(),
                                probe_name: probe_payload.id.clone(),
                                port: port.number,
                                transport: port.transport,
                                payload: probe_payload.payload.clone(),
                                payload_encoding: probe_payload.payload_encoding,
                            };
                            let ctx = ProbeContext {
                                ip,
                                hostname: hostname.clone(),
                                probe: port_probe,
                                timeout: config.timeout,
                                max_read_size: config.max_read_size,
                                sni: config.sni,
                                skip_cert_verify: config.skip_cert_verify,
                            };

                            let r = match probe {
                                ServiceProbe::TcpHTTPGet
                                | ServiceProbe::TcpHTTPSGet
                                | ServiceProbe::TcpHTTPOptions => {
                                    probe::http::HttpProbe::run(ctx).await
                                }
                                ServiceProbe::TcpImapCapability
                                | ServiceProbe::TcpMssqlPrelogin
                                | ServiceProbe::TcpMemcachedVersion
                                | ServiceProbe::TcpMqttConnect
                                | ServiceProbe::TcpMySqlHandshake
                                | ServiceProbe::TcpOracleTns
                                | ServiceProbe::TcpPop3Capa
                                | ServiceProbe::TcpPostgresSslReq
                                | ServiceProbe::TcpRedisPing
                                | ServiceProbe::TcpRdpNegotiation
                                | ServiceProbe::TcpSmbNegotiation
                                | ServiceProbe::TcpSmtpEhlo => {
                                    probe::special::SpecialTcpProbe::run(ctx).await
                                }
                                ServiceProbe::TcpTlsSession => probe::tls::TlsProbe::run(ctx).await,
                                ServiceProbe::TcpGenericLines | ServiceProbe::TcpHelp => {
                                    probe::generic::GenericProbe::run(ctx).await
                                }
                                ServiceProbe::UdpDNSVersionBindReq
                                | ServiceProbe::TcpDNSVersionBindReq => {
                                    probe::dns::DnsProbe::run(ctx).await
                                }
                                ServiceProbe::UdpQuic => probe::quic::QuicProbe::run(ctx).await,
                                _ => probe::null::NullProbe::run(ctx).await,
                            };
                            results.push(r);
                        }
                    } else {
                        let ctx = ProbeContext {
                            ip,
                            hostname: hostname.clone(),
                            probe: PortProbe::null_probe(port.number, port.transport),
                            timeout: config.timeout,
                            max_read_size: config.max_read_size,
                            sni: config.sni,
                            skip_cert_verify: config.skip_cert_verify,
                        };
                        results.push(probe::null::NullProbe::run(ctx).await);
                    }
                    results
                }
            })
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await;

        let mut results = Vec::new();
        for result in batches.into_iter().flatten() {
            match result {
                Ok(result) => results.push(result),
                Err(error) => tracing::debug!(%error, "service probe failed"),
            }
        }
        Ok(results)
    }

    pub async fn run_service_detection(
        &self,
        targets: Vec<Endpoint>,
    ) -> Result<ServiceDetectionResult> {
        let start_time = Instant::now();
        // Keep probe futures owned by this call so cancellation drops their sockets.
        let mut tasks = stream::iter(targets)
            .map(|endpoint| Self::detect_services(self.config.clone(), endpoint))
            .buffer_unordered(self.config.max_concurrency.max(1));
        let mut results = Vec::new();
        while let Some(result) = tasks.next().await {
            match result {
                Ok(mut batch) => results.append(&mut batch),
                Err(error) => tracing::error!(%error, "service detection failed"),
            }
        }
        Ok(ServiceDetectionResult {
            results,
            scan_time: start_time.elapsed(),
        })
    }
}

#[allow(dead_code)]
pub fn set_read_timeout(tcp_stream: TcpStream, timeout: Duration) -> std::io::Result<TcpStream> {
    // Convert to std::net::TcpStream
    let std_tcp_stream = tcp_stream.into_std()?;
    // Set read timeout
    std_tcp_stream.set_read_timeout(Some(timeout))?;
    // Convert back to tokio TcpStream
    let tokio_tcp_stream = TcpStream::from_std(std_tcp_stream)?;
    Ok(tokio_tcp_stream)
}

pub async fn read_timeout<S>(
    reader: &mut S,
    idle_timeout: Duration,
    total_timeout: Duration,
    max_bytes: usize,
) -> Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    let start = Instant::now();
    let mut buf = [0u8; 4096];
    let mut out = Vec::new();

    loop {
        // Check total timeout
        let elapsed = start.elapsed();
        if elapsed >= total_timeout {
            break;
        }
        let remaining_total = total_timeout - elapsed;
        let wait = idle_timeout.min(remaining_total);

        match timeout(wait, reader.read(&mut buf)).await {
            // Closed by peer
            Ok(Ok(0)) => break,
            // Data read
            Ok(Ok(n)) => {
                if n > max_bytes.saturating_sub(out.len()) {
                    bail!(
                        "response exceeded max_bytes ({} > {})",
                        out.len() + n,
                        max_bytes
                    );
                }
                out.extend_from_slice(&buf[..n]);

                continue;
            }
            // Read error
            Ok(Err(e)) => bail!("error reading response: {e}"),
            // Idle timeout (no data received)
            Err(_elapsed) => break,
        }
    }

    if out.is_empty() {
        bail!("no response within time limits");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::endpoint::{Port, TransportProtocol};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn response_limit_includes_the_last_chunk() {
        let duration = Duration::from_secs(1);
        assert!(read_timeout(&mut &b"abcde"[..], duration, duration, 4)
            .await
            .is_err());
        assert_eq!(
            read_timeout(&mut &b"abcd"[..], duration, duration, 4)
                .await
                .unwrap(),
            b"abcd"
        );
    }

    #[tokio::test]
    async fn cancelling_service_detection_closes_active_connections() {
        crate::command::scan::init_probe_db().await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut endpoint = Endpoint::new(address.ip());
        endpoint.upsert_port(Port::new(address.port(), TransportProtocol::Tcp));
        let detector = ServiceDetector::new(ServiceProbeConfig {
            timeout: Duration::from_secs(30),
            max_concurrency: 1,
            max_read_size: 1024,
            sni: true,
            skip_cert_verify: true,
        });
        let task =
            tokio::spawn(async move { detector.run_service_detection(vec![endpoint]).await });
        let (mut peer, _) = timeout(Duration::from_secs(2), listener.accept())
            .await
            .unwrap()
            .unwrap();
        task.abort();
        let Err(error) = task.await else {
            panic!("probe completed before cancellation");
        };
        assert!(error.is_cancelled());
        let mut received = Vec::new();
        if let Err(error) = timeout(Duration::from_secs(2), peer.read_to_end(&mut received))
            .await
            .expect("probe socket survived cancellation")
        {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ));
        }
    }
}
