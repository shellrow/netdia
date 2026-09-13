use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::probe::service::db::service::{tcp_service_db, udp_service_db};
use crate::probe::service::models::ServiceInfo;
use crate::probe::service::probe::{PortProbeResult, ProbeContext, ServiceProbe};
use anyhow::Result;

use hickory_proto::{
    op::{Message, MessageType, OpCode, Query},
    rr::{DNSClass, Name, RecordType},
    serialize::binary::{BinEncodable, BinEncoder},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
};

/// Build a DNS query message for "version.bind" TXT record in CHAOS class.
fn build_version_bind_query() -> anyhow::Result<Vec<u8>> {
    let mut msg = Message::new(fastrand::u16(..), MessageType::Query, OpCode::Query);
    msg.metadata.recursion_desired = false;

    let name = Name::from_ascii("version.bind.")?;
    let mut q = Query::query(name, RecordType::TXT);
    // CHAOS class for version.bind
    q.set_query_class(DNSClass::CH);
    msg.add_query(q);

    let mut bytes = Vec::with_capacity(64);
    let mut enc = BinEncoder::new(&mut bytes);
    msg.emit(&mut enc)?;
    Ok(bytes)
}

/// Perform a DNS version.bind query over UDP.
async fn run_dns_version_bind_udp(
    addr: std::net::SocketAddr,
    idle: std::time::Duration,
    _total: std::time::Duration,
    max_bytes: usize,
) -> anyhow::Result<(String, bool)> {
    let q = build_version_bind_query()?;
    let local = if addr.is_ipv6() {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)
    } else {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
    };
    let sock = UdpSocket::bind(local).await?;
    sock.connect(addr).await?;
    sock.send(&q).await?;

    // Receive response
    let mut buf = vec![0u8; max_bytes.min(4096)];
    let n = tokio::time::timeout(idle, sock.recv(&mut buf)).await??;
    buf.truncate(n);

    let msg = Message::from_vec(&buf)?;
    let truncated = msg.metadata.truncation;

    // Extract TXT record
    let mut txt = String::new();
    for ans in &msg.answers {
        if ans.data.record_type() == RecordType::TXT
            && ans.name.to_ascii().eq_ignore_ascii_case("version.bind.")
            && ans.dns_class == DNSClass::CH
        {
            if let hickory_proto::rr::RData::TXT(t) = &ans.data {
                let joined = t
                    .txt_data
                    .iter()
                    .map(|b| String::from_utf8_lossy(b).to_string())
                    .collect::<Vec<_>>()
                    .join("");
                txt = joined;
                break;
            }
        }
    }

    if txt.is_empty() {
        anyhow::bail!("no TXT answer for version.bind");
    }
    Ok((txt, truncated))
}

/// Perform a DNS version.bind query over TCP.
async fn run_dns_version_bind_tcp(
    addr: std::net::SocketAddr,
    idle: std::time::Duration,
    total: std::time::Duration,
    max_bytes: usize,
) -> anyhow::Result<String> {
    let mut stream = tokio::time::timeout(total, TcpStream::connect(addr)).await??;

    let q = build_version_bind_query()?;
    let mut framed = Vec::with_capacity(q.len() + 2);
    framed.extend_from_slice(&(q.len() as u16).to_be_bytes());
    framed.extend_from_slice(&q);
    stream.write_all(&framed).await?;
    stream.flush().await?;

    // Read the first 2 bytes for length
    let mut lenbuf = [0u8; 2];
    tokio::time::timeout(idle, stream.read_exact(&mut lenbuf)).await??;
    let want = u16::from_be_bytes(lenbuf) as usize;
    if want > max_bytes {
        anyhow::bail!("dns/tcp response exceeds max_bytes");
    }

    let mut buf = vec![0u8; want];
    tokio::time::timeout(idle, stream.read_exact(&mut buf)).await??;

    let msg = hickory_proto::op::Message::from_vec(&buf)?;
    // Extract TXT record
    for ans in &msg.answers {
        if ans.data.record_type() == RecordType::TXT
            && ans.name.to_ascii().eq_ignore_ascii_case("version.bind.")
            && ans.dns_class == DNSClass::CH
        {
            if let hickory_proto::rr::RData::TXT(t) = &ans.data {
                let joined = t
                    .txt_data
                    .iter()
                    .map(|b| String::from_utf8_lossy(b).to_string())
                    .collect::<Vec<_>>()
                    .join("");
                if !joined.is_empty() {
                    return Ok(joined);
                }
            }
        }
    }
    anyhow::bail!("no TXT answer for version.bind over TCP");
}

/// A DNS probe that performs version.bind queries to identify DNS services.
pub struct DnsProbe;

impl DnsProbe {
    /// Run the DNS probe with the given context.
    pub async fn run(ctx: ProbeContext) -> Result<PortProbeResult> {
        let addr = std::net::SocketAddr::new(ctx.ip, ctx.probe.port);

        // Try UDP first
        if matches!(
            ctx.probe.probe_id,
            ServiceProbe::UdpDNSVersionBindReq | ServiceProbe::TcpDNSVersionBindReq
        ) {
            tracing::debug!(
                "DNS Version Bind Probe (UDP): {}:{}",
                ctx.ip,
                ctx.probe.port
            );
            match run_dns_version_bind_udp(addr, ctx.timeout, ctx.timeout, ctx.max_read_size).await
            {
                Ok((txt, truncated)) => {
                    let mut svc = ServiceInfo::default();
                    let udp_svc_db = udp_service_db();
                    svc.name = udp_svc_db.get_name(ctx.probe.port).map(|s| s.to_string());
                    svc.banner = Some(txt.clone());
                    svc.raw = Some(txt.clone());
                    // If truncated, try TCP as well
                    if truncated {
                        if let Ok(txt2) = run_dns_version_bind_tcp(
                            addr,
                            ctx.timeout,
                            ctx.timeout,
                            ctx.max_read_size,
                        )
                        .await
                        {
                            svc.raw = Some(txt2.clone());
                            svc.banner = Some(txt2);
                        }
                    }
                    let probe_result: PortProbeResult = PortProbeResult {
                        ip: ctx.ip,
                        hostname: ctx.hostname,
                        port: ctx.probe.port,
                        transport: ctx.probe.transport,
                        probe_id: ctx.probe.probe_id,
                        service_info: svc,
                    };
                    return Ok(probe_result);
                }
                Err(e) => {
                    tracing::debug!("DNS Version Bind Probe (UDP) failed: {}", e);
                    tracing::debug!(
                        "Attempting DNS Version Bind Probe (TCP): {}:{}",
                        ctx.ip,
                        ctx.probe.port
                    );
                    let mut svc = ServiceInfo::default();
                    let tcp_svc_db = tcp_service_db();
                    svc.name = tcp_svc_db.get_name(ctx.probe.port).map(|s| s.to_string());
                    // If UDP failed, try TCP
                    if let Ok(txt) =
                        run_dns_version_bind_tcp(addr, ctx.timeout, ctx.timeout, ctx.max_read_size)
                            .await
                    {
                        svc.banner = Some(txt.clone());
                        svc.raw = Some(txt.clone());
                    }
                    let probe_result: PortProbeResult = PortProbeResult {
                        ip: ctx.ip,
                        hostname: ctx.hostname,
                        port: ctx.probe.port,
                        transport: ctx.probe.transport,
                        probe_id: ctx.probe.probe_id,
                        service_info: svc,
                    };
                    return Ok(probe_result);
                }
            }
        }

        // If UDP not selected or failed, and TCP is selected
        if matches!(ctx.probe.probe_id, ServiceProbe::TcpDNSVersionBindReq) {
            tracing::debug!(
                "DNS Version Bind Probe (TCP): {}:{}",
                ctx.ip,
                ctx.probe.port
            );
            let txt =
                run_dns_version_bind_tcp(addr, ctx.timeout, ctx.timeout, ctx.max_read_size).await?;
            let mut svc = ServiceInfo::default();
            let tcp_svc_db = tcp_service_db();
            svc.name = tcp_svc_db.get_name(ctx.probe.port).map(|s| s.to_string());
            svc.banner = Some(txt.clone());
            svc.raw = Some(txt.clone());
            let probe_result: PortProbeResult = PortProbeResult {
                ip: ctx.ip,
                hostname: ctx.hostname,
                port: ctx.probe.port,
                transport: ctx.probe.transport,
                probe_id: ctx.probe.probe_id,
                service_info: svc,
            };
            return Ok(probe_result);
        }
        anyhow::bail!("unsupported probe for dns version.bind")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::{rdata::TXT, RData, Record};
    use std::time::Duration;
    use tokio::net::TcpListener;

    const TIMEOUT: Duration = Duration::from_secs(2);

    fn response(query: &[u8], truncated: bool, valid_answer: bool) -> Vec<u8> {
        let query = Message::from_vec(query).unwrap();
        assert_eq!(query.metadata.message_type, MessageType::Query);
        assert!(!query.metadata.recursion_desired);
        assert_eq!(query.queries.len(), 1);
        assert_eq!(query.queries[0].name().to_ascii(), "version.bind.");
        assert_eq!(query.queries[0].query_type(), RecordType::TXT);
        assert_eq!(query.queries[0].query_class(), DNSClass::CH);
        let mut message = Message::response(query.metadata.id, OpCode::Query);
        message.metadata.truncation = truncated;
        message.add_queries(query.queries);
        let mut record = Record::from_rdata(
            Name::from_ascii("VERSION.BIND.").unwrap(),
            0,
            RData::TXT(TXT::new(vec!["Test DNS ".into(), "1.0".into()])),
        );
        // An IN-class record must never be mistaken for the CHAOS-class banner.
        message.add_answer(record.clone());
        if valid_answer {
            record.dns_class = DNSClass::CH;
            message.add_answer(record);
        }
        message.to_vec().unwrap()
    }

    #[tokio::test]
    async fn udp_probe_preserves_txt_segments_and_truncation_flag() {
        for truncated in [false, true] {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let addr = socket.local_addr().unwrap();
            let server = async {
                let mut buf = [0; 512];
                let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
                socket
                    .send_to(&response(&buf[..n], truncated, true), peer)
                    .await
                    .unwrap();
            };
            let client = run_dns_version_bind_udp(addr, TIMEOUT, TIMEOUT, 4096);
            let (_, result) = tokio::time::timeout(TIMEOUT, async { tokio::join!(server, client) })
                .await
                .unwrap();
            assert_eq!(result.unwrap(), ("Test DNS 1.0".into(), truncated));
        }
    }

    #[tokio::test]
    async fn udp_probe_rejects_wrong_class_and_malformed_responses() {
        for malformed in [false, true] {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let addr = socket.local_addr().unwrap();
            let server = async {
                let mut buf = [0; 512];
                let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
                let bytes = if malformed {
                    vec![0, 1, 2]
                } else {
                    response(&buf[..n], false, false)
                };
                socket.send_to(&bytes, peer).await.unwrap();
            };
            let client = run_dns_version_bind_udp(addr, TIMEOUT, TIMEOUT, 4096);
            let (_, result) = tokio::time::timeout(TIMEOUT, async { tokio::join!(server, client) })
                .await
                .unwrap();
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn tcp_probe_handles_fragmented_frames_and_enforces_size_limit() {
        for oversized in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = async {
                let (mut stream, _) = listener.accept().await.unwrap();
                let len = stream.read_u16().await.unwrap() as usize;
                let mut query = vec![0; len];
                stream.read_exact(&mut query).await.unwrap();
                let bytes = response(&query, false, true);
                let size = if oversized { 4097 } else { bytes.len() as u16 };
                stream.write_u16(size).await.unwrap();
                if !oversized {
                    for chunk in bytes.chunks(3) {
                        stream.write_all(chunk).await.unwrap();
                        tokio::task::yield_now().await;
                    }
                }
            };
            let client = run_dns_version_bind_tcp(addr, TIMEOUT, TIMEOUT, 4096);
            let (_, result) = tokio::time::timeout(TIMEOUT, async { tokio::join!(server, client) })
                .await
                .unwrap();
            if oversized {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("exceeds max_bytes"));
            } else {
                assert_eq!(result.unwrap(), "Test DNS 1.0");
            }
        }
    }
}
