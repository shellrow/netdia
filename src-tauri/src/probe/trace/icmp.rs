use crate::model::trace::{TraceCancelledPayload, TraceHop, TracerouteSetting};
use crate::probe::packet::build_icmp_echo_bytes;
use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
use anyhow::Result;
use nex_packet::icmp::{IcmpPacket, IcmpType};
use nex_packet::icmpv6::{Icmpv6Packet, Icmpv6Type};
use nex_packet::ip::IpNextProtocol;
use nex_packet::ipv4::Ipv4Packet;
use nex_packet::packet::Packet;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

/// Check if the ICMP packet is an Echo Reply for the given destination IP
fn is_echo_reply(dst_ip: IpAddr, icmp_bytes: &[u8]) -> bool {
    match dst_ip {
        IpAddr::V4(_) => {
            if let Some(ip) = Ipv4Packet::from_buf(icmp_bytes) {
                if ip.header.next_level_protocol == IpNextProtocol::Icmp {
                    if let Some(icmp) = IcmpPacket::from_bytes(ip.payload()) {
                        return matches!(icmp.header.icmp_type, IcmpType::EchoReply);
                    }
                }
            }
            false
        }
        IpAddr::V6(_) => {
            if let Some(icmp6) = Icmpv6Packet::from_buf(icmp_bytes) {
                return matches!(icmp6.header.icmpv6_type, Icmpv6Type::EchoReply);
            }
            false
        }
    }
}

/// ICMP Echo based traceroute
///
/// - Increase TTL/HopLimit from 1 to max_hops for each hop
/// - Send tries_per_hop times for each hop and summarize the best RTT in `TraceHop`
/// - If an Echo Reply is received from the destination, end with `reached = true`
pub async fn icmp_traceroute(
    app: &AppHandle,
    run_id: &str,
    src_ip: IpAddr,
    setting: &TracerouteSetting,
    token: CancellationToken,
) -> Result<bool> {
    let dst_ip = setting.ip_addr;
    let icmp_kind = if dst_ip.is_ipv4() {
        IcmpKind::V4
    } else {
        IcmpKind::V6
    };

    let timeout = Duration::from_millis(setting.timeout_ms);
    // TODO: echo_id should be randomized per run
    let echo_id: u16 = 0x1234;
    let payload = b"netd";

    // Whether reached the destination at any hop
    let mut reached = false;

    'ttl_loop: for ttl in 1..=setting.max_hops {
        if token.is_cancelled() {
            let _ = app.emit("traceroute:cancelled", run_id.to_string());
            return Err(anyhow::anyhow!("cancelled"));
        }
        // Create socket for each TTL/HopLimit
        let mut cfg = IcmpConfig::new(icmp_kind);
        if dst_ip.is_ipv4() {
            cfg = cfg.with_ttl(ttl as u32);
            if let IpAddr::V4(v4) = src_ip {
                cfg = cfg.with_bind(SocketAddr::new(IpAddr::V4(v4), 0));
            }
        } else {
            cfg = cfg.with_hoplimit(ttl as u32);
            if let IpAddr::V6(v6) = src_ip {
                cfg = cfg.with_bind(SocketAddr::new(IpAddr::V6(v6), 0));
            }
        }

        let socket = AsyncIcmpSocket::new(&cfg).await?;
        let target = SocketAddr::new(dst_ip, 0);

        let mut best: TraceHop = TraceHop {
            hop: ttl,
            ip_addr: None,
            rtt_ms: None,
            reached: false,
            note: None,
        };

        for t in 0..setting.tries_per_hop {
            let seq = ((ttl as u16) << 8) | (t as u16);
            let pkt = build_icmp_echo_bytes(src_ip, dst_ip, echo_id, seq, payload);

            let sent_at = Instant::now();

            // Send ICMP Echo Request
            if let Err(e) = socket.send_to(&pkt, target).await {
                // if send error, record and break immediately
                best.note = Some(format!("send error: {e}"));
                break;
            }

            // Recv (with timeout)
            let mut buf = vec![0u8; 2048];
            let res = tokio::time::timeout(timeout, socket.recv_from(&mut buf)).await;

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

                    // Use this response as the representative for this hop (only adopt smaller RTT)
                    if best.rtt_ms.map_or(true, |cur| rtt < cur) {
                        best.rtt_ms = Some(rtt);
                        best.ip_addr = Some(from_ip);
                        best.note = None;
                    }

                    // Check if the ICMP packet is an Echo Reply from the destination
                    if is_echo_reply(dst_ip, &buf[..n]) {
                        best.reached = true;
                        reached = true;
                        // Emit this hop as progress and break the ttl_loop
                        app.emit("traceroute:progress", &best).ok();
                        break 'ttl_loop;
                    }
                }
            }
        }

        // If no response, treat as timeout
        if best.ip_addr.is_none() && best.note.is_none() {
            best.note = Some("timeout".into());
        }

        app.emit("traceroute:progress", &best).ok();

        if token.is_cancelled() {
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
