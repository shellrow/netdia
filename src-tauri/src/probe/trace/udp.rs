#![allow(unused)]

use anyhow::Result;
use bytes::Bytes;
use nex_packet::icmp::IcmpPacket;
use nex_packet::icmp::IcmpType;
use nex_packet::icmpv6::Icmpv6Packet;
use nex_packet::icmpv6::Icmpv6Type;
use nex_packet::ip::IpNextProtocol;
use nex_packet::ipv4::Ipv4Packet;
use nex_packet::packet::Packet;
#[cfg(unix)]
use tokio_util::sync::CancellationToken;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
#[cfg(unix)]
use crate::model::trace::{TracerouteSetting, TraceCancelledPayload};
use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
use crate::socket::udp::{AsyncUdpSocket, UdpConfig};
use crate::socket::SocketFamily;

/// Default base target UDP port for traceroute
const DEFAULT_BASE_TARGET_UDP_PORT: u16 = 33435;

/// Check if the ICMPv4 is a Destination Unreachable for the given destination IP
fn is_port_unreach_v4(icmp_bytes: &[u8]) -> bool {
    if let Some(ip) = Ipv4Packet::from_buf(icmp_bytes) {
        if ip.header.next_level_protocol == IpNextProtocol::Icmp {
            if let Some(icmp) = IcmpPacket::from_bytes(ip.payload()) {
                return matches!(icmp.header.icmp_type, IcmpType::DestinationUnreachable);
            }
        }
    }
    false
}

/// Check if the ICMPv6 is a Destination Unreachable for the given destination IP
fn is_port_unreach_v6(icmp_bytes: &[u8]) -> bool {
    if let Some(icmp6) = Icmpv6Packet::from_buf(icmp_bytes) {
        return matches!(icmp6.header.icmpv6_type, Icmpv6Type::DestinationUnreachable);
    }
    false
}

#[cfg(unix)]
pub async fn udp_traceroute(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: &TracerouteSetting,
    token: CancellationToken
) -> Result<bool> {
    let dst_ip = setting.ip_addr;
    let timeout = Duration::from_millis(setting.timeout_ms);

    // ICMP socket to receive Port Unreachable messages
    let icmp_kind = if dst_ip.is_ipv4() {
        IcmpKind::V4
    } else {
        IcmpKind::V6
    };
    let icmp = AsyncIcmpSocket::new(&IcmpConfig::new(icmp_kind)).await?;

    let mut reached = false;

    'ttl_loop: for ttl in 1..=setting.max_hops {
        use crate::model::trace::TraceHop;

        if token.is_cancelled() {
            let _ = app.emit(
                "traceroute:cancelled",
                run_id.to_string(),
            );
            return Err(anyhow::anyhow!("cancelled"));
        }
        let mut ucfg = UdpConfig::new();
        ucfg.socket_family = SocketFamily::from_ip(&dst_ip);

        if dst_ip.is_ipv4() {
            ucfg.ttl = Some(ttl as u32);
            ucfg.bind_addr = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
        } else {
            ucfg.hoplimit = Some(ttl as u32);
            ucfg.bind_addr = Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0));
        }

        let udp = AsyncUdpSocket::from_config(&ucfg)?;
        let local_addr = udp.local_addr()?;

        let mut best = TraceHop {
            hop: ttl,
            ip_addr: None,
            rtt_ms: None,
            reached: false,
            note: None,
        };

        for t in 0..setting.tries_per_hop {
            let dst_port = DEFAULT_BASE_TARGET_UDP_PORT
                .wrapping_add(ttl as u16)
                .wrapping_add(t as u16);
            let target = SocketAddr::new(dst_ip, dst_port);
            let payload = Bytes::from_static(b"netd");

            let sent_at = Instant::now();

            if let Err(e) = udp.send_to(&payload, target).await {
                best.note = Some(format!("send error: {e}"));
                break;
            }

            let mut buf = vec![0u8; 2048];
            let res = tokio::time::timeout(timeout, icmp.recv_from(&mut buf)).await;

            match res {
                Err(_) => {
                    // timeout -> continue with remaining tries_per_hop
                    continue;
                }
                Ok(Err(e)) => {
                    best.note = Some(format!("recv error: {e}"));
                    break;
                }
                Ok(Ok((n, from))) => {
                    let rtt = sent_at.elapsed().as_millis() as u64;
                    let from_ip = from.ip();

                    if best.rtt_ms.map_or(true, |cur| rtt < cur) {
                        best.rtt_ms = Some(rtt);
                        best.ip_addr = Some(from_ip);
                        best.note = None;
                    }

                    // Strictly speaking, we should check if the response is for the UDP we sent.
                    // By checking the embedded IP/UDP headers in the ICMP payload.
                    // For simplicity, we skip that here.
                    let is_dest = match (dst_ip, local_addr.ip()) {
                        (IpAddr::V4(_d), IpAddr::V4(_s)) => is_port_unreach_v4(&buf[..n]),
                        (IpAddr::V6(_d), IpAddr::V6(_s)) => is_port_unreach_v6(&buf[..n]),
                        _ => false,
                    };

                    if is_dest {
                        best.reached = true;
                        reached = true;
                        app.emit("traceroute:progress", &best).ok();
                        break 'ttl_loop;
                    }
                }
            }
        }

        if best.ip_addr.is_none() && best.note.is_none() {
            best.note = Some("timeout".into());
        }

        app.emit("traceroute:progress", &best).ok();

        if token.is_cancelled() {
            use crate::model::trace::TraceCancelledPayload;

            let _ = app.emit(
                "traceroute:cancelled",
                TraceCancelledPayload {
                    run_id: run_id.to_string(),
                },
            );
            return Err(anyhow::anyhow!("cancelled"));
        }
    }

    Ok(reached)
}

#[cfg(windows)]
pub async fn udp_traceroute(
    _app: &AppHandle,
    _run_id: &str,
    _src_ip: IpAddr,
    _setting: &TracerouteSetting,
    _token: CancellationToken
) -> Result<bool> {
    // Currently, windows is not supported for UDP traceroute via ICMP Port Unreachable
    // because it requires enabling promiscuous mode on ICMP socket.
    // and it needs admin privileges.
    // For cross-platform, non-admin, and not rely on npcap/winpcap, we skip implementing this feature on Windows.
    // For now, just return an error.
    Err(anyhow::anyhow!(
        "UDP traceroute is not supported on Windows (ICMP capture limitation)."
    ))
}
