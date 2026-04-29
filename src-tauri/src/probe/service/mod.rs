use anyhow::{bail, Result};
use futures::stream::{self, StreamExt};
use probe::{PortProbe, PortProbeResult, ProbeContext, ServiceProbe};
use std::time::Duration;
use tokio::sync::mpsc;
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
        let (ch_tx, mut ch_rx) = mpsc::unbounded_channel::<Vec<Result<PortProbeResult>>>();

        let mut results = Vec::new();
        let recv_task = tokio::spawn(async move {
            while let Some(port_results) = ch_rx.recv().await {
                for res in port_results {
                    match res {
                        Ok(r) => results.push(r),
                        Err(e) => tracing::debug!("Probe failed: {}", e),
                    }
                }
            }
            results
        });

        let ports = endpoint.ports.clone();
        let prod = stream::iter(ports).for_each_concurrent(config.max_concurrency, move |port| {
            let tx = ch_tx.clone();
            let endpoint = endpoint.clone();
            let port_probe_db = port_probe_db.clone();
            let service_probe_db = service_probe_db.clone();
            async move {
                // Perform service detection for each endpoint
                let mut results: Vec<Result<PortProbeResult>> = Vec::new();
                if let Some(probes) = port_probe_db.get(&port) {
                    for probe in probes {
                        let probe_payload = match service_probe_db.get(probe) {
                            Some(payload) => payload,
                            None => {
                                results
                                    .push(Err(anyhow::anyhow!("No payload for probe {:?}", probe)));
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
                            ip: endpoint.ip,
                            hostname: endpoint.hostname.clone(),
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
                        ip: endpoint.ip,
                        hostname: endpoint.hostname.clone(),
                        probe: PortProbe::null_probe(port.number, port.transport),
                        timeout: config.timeout,
                        max_read_size: config.max_read_size,
                        sni: config.sni,
                        skip_cert_verify: config.skip_cert_verify,
                    };
                    results.push(probe::null::NullProbe::run(ctx).await);
                }
                let _ = tx.send(results);
            }
        });

        let prod_task = tokio::spawn(prod);
        let (results_res, _prod_res) = tokio::join!(recv_task, prod_task);
        let results = results_res?;
        Ok(results)
    }

    pub async fn run_service_detection(
        &self,
        targets: Vec<Endpoint>,
    ) -> Result<ServiceDetectionResult> {
        let start_time = Instant::now();
        let mut tasks = vec![];
        for endpoint in targets {
            let endpoint = endpoint.clone();
            let conf = self.config.clone();
            tasks.push(tokio::spawn(async move {
                let probe_results = Self::detect_services(conf, endpoint).await;
                probe_results
            }));
        }
        let mut results: Vec<PortProbeResult> = Vec::new();
        for task in tasks {
            if let Ok(r) = task.await {
                match r {
                    Ok(mut result) => {
                        // Merge results
                        results.append(&mut result);
                    }
                    Err(e) => {
                        tracing::error!("Service detection failed: {}", e);
                    }
                }
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
                if out.len() > max_bytes {
                    bail!(
                        "response exceeded max_bytes ({} > {})",
                        out.len(),
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
