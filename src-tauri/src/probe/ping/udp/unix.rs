use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use bytes::Bytes;
use nex_packet::icmp::IcmpType;
use nex_packet::icmpv6::Icmpv6Packet;
use nex_packet::icmpv6::Icmpv6Type;
use nex_packet::packet::Packet;
use nex_packet::{icmp::IcmpPacket, ip::IpNextProtocol, ipv4::Ipv4Packet};
use tauri::{AppHandle, Emitter};
use crate::model::probe::{ProbeStatus, ProbeStatusKind};
use crate::model::ping::PingCancelledPayload;
use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
use crate::socket::udp::{AsyncUdpSocket, UdpConfig};
use crate::socket::SocketFamily;
use crate::model::ping::{
    PingDonePayload, PingProgressPayload, PingProtocol, PingSample, PingSetting, PingStat,
};

/// Default base target UDP port for traceroute or ping
const DEFAULT_BASE_TARGET_UDP_PORT: u16 = 33435;

/// Check if the ICMP message is Port Unreachable
fn is_port_unreach_v4(icmp_bytes: &[u8]) -> bool {
    if let Some(ip) = Ipv4Packet::from_buf(icmp_bytes) {
        if ip.header.next_level_protocol == IpNextProtocol::Icmp {
            if let Some(icmp) = IcmpPacket::from_bytes(ip.payload()) {
                match icmp.header.icmp_type {
                    IcmpType::DestinationUnreachable => {
                        return true;
                    }
                    _ => return false,
                }
            }
        }
    }
    false
}

/// Check if the ICMPv6 message is Port Unreachable
/// The IPv6 header is automatically cropped off when recvfrom() is used.
fn is_port_unreach_v6(icmp_bytes: &[u8]) -> bool {
    if let Some(icmp6) = Icmpv6Packet::from_buf(icmp_bytes) {
        match icmp6.header.icmpv6_type {
            Icmpv6Type::DestinationUnreachable => {
                return true;
            }
            _ => return false,
        }
    }
    false
}

/// UDP Ping using ICMP Port Unreachable messages
pub async fn udp_ping_icmp_unreach(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: PingSetting,
    token: CancellationToken
) -> Result<PingStat> {
    let dst_ip = setting.ip_addr;
    //let dst_port = setting.port.unwrap_or(DEFAULT_BASE_TARGET_UDP_PORT);
    let dst_port = DEFAULT_BASE_TARGET_UDP_PORT;
    let target = SocketAddr::new(dst_ip, dst_port);
    // UDP Socket for sending UDP packets
    let mut ucfg = UdpConfig::new();
    ucfg.socket_family = SocketFamily::from_ip(&dst_ip);
    if dst_ip.is_ipv4() {
        ucfg.ttl = Some(setting.hop_limit as u32);
        ucfg.bind_addr = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
    } else {
        ucfg.hoplimit = Some(setting.hop_limit as u32);
        ucfg.bind_addr = Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0));
    }
    let udp = AsyncUdpSocket::from_config(&ucfg)?;

    let local_addr = udp.local_addr()?;

    // ICMP Socket for receiving ICMP Port Unreachable messages
    let icmp_kind = if dst_ip.is_ipv4() {
        IcmpKind::V4
    } else {
        IcmpKind::V6
    };
    let icmp = AsyncIcmpSocket::new(&IcmpConfig::new(icmp_kind)).await?;

    let mut samples = Vec::with_capacity(setting.count as usize);
    let mut rtts = Vec::<u64>::new();

    let mut received = 0u32;

    for seq in 1..=setting.count {
        if token.is_cancelled() {
            let _ = app.emit(
        "ping:cancelled",
        PingCancelledPayload {
                    run_id: run_id.to_string(),
                },
            );
            return Err(anyhow::anyhow!("cancelled"));
        }
        let payload = Bytes::from_static(b"netd");
        let sent_at = Instant::now();

        let mut status = ProbeStatus::new();
        let mut rtt_ms = None;

        // Send UDP packet
        if let Err(e) = udp.send_to(&payload, target).await {
            status = ProbeStatus::with_error_message(format!("send error: {e}"));
        } else {
            // Wait for ICMP PortUnreachable response (with timeout)
            let to = Duration::from_millis(setting.timeout_ms);
            let mut buf = vec![0u8; 2048];

            let matched = tokio::time::timeout(to, async {
                loop {
                    let (n, _from) = icmp.recv_from(&mut buf).await?;
                    let ok = match (dst_ip, local_addr.ip()) {
                        (IpAddr::V4(_d), IpAddr::V4(_s)) => is_port_unreach_v4(&buf[..n]),
                        (IpAddr::V6(_d), IpAddr::V6(_s)) => is_port_unreach_v6(&buf[..n]),
                        _ => false,
                    };
                    if ok {
                        break Ok::<_, std::io::Error>(());
                    }
                }
            })
            .await
            .is_ok();

            if matched {
                let rtt = sent_at.elapsed().as_millis() as u64;
                rtt_ms = Some(rtt);
                rtts.push(rtt);
            } else {
                status = ProbeStatus::with_timeout_message(format!(
                    "timeout (>{}ms)",
                    setting.timeout_ms
                ));
            }
        }

        let sample = PingSample {
            seq,
            ip_addr: dst_ip,
            hostname: setting.hostname.clone(),
            port: Some(dst_port),
            rtt_ms,
            probe_status: status,
            protocol: PingProtocol::Udp,
        };

        if sample.rtt_ms.is_some() && matches!(sample.probe_status.kind, ProbeStatusKind::Done) {
            received += 1;
        }

        let transmitted = seq;
        let percent = (seq as f32) * 100.0 / (setting.count as f32);

        let _ = app.emit(
            "ping:progress",
            PingProgressPayload {
                run_id: run_id.to_string(),
                sample: sample.clone(),
                transmitted,
                received,
                percent,
            },
        );
        samples.push(sample);

        if seq != setting.count {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(setting.send_rate_ms)) => {}
                _ = token.cancelled() => {
                    let _ = app.emit(
                        "ping:cancelled",
                        PingCancelledPayload {
                            run_id: run_id.to_string(),
                        },
                    );
                    return Err(anyhow::anyhow!("cancelled"));
                }
            }
        }
    }

    // Summarize samples
    let (min, avg, max) = if rtts.is_empty() {
        (None, None, None)
    } else {
        let mut min = u64::MAX;
        let mut max = 0;
        let mut sum: u128 = 0;
        for &v in &rtts {
            if v < min {
                min = v
            };
            if v > max {
                max = v
            };
            sum += v as u128;
        }
        (
            Some(min),
            Some((sum / (rtts.len() as u128)) as u64),
            Some(max),
        )
    };

    let stat = PingStat {
        ip_addr: dst_ip,
        hostname: setting.hostname.clone(),
        port: Some(dst_port),
        protocol: PingProtocol::Udp,
        samples: samples.clone(),
        transmitted_count: samples.len(),
        received_count: samples
            .iter()
            .filter(|s| s.rtt_ms.is_some() && s.probe_status.kind == ProbeStatusKind::Done)
            .count(),
        min,
        avg,
        max,
    };

    // Send done event
    let _ = app.emit(
        "ping:done",
        PingDonePayload {
            run_id: run_id.to_string(),
            stat: stat.clone(),
        },
    );

    Ok(stat)
}
