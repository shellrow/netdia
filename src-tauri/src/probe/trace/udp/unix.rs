use crate::events::EventEmitter;
use crate::model::trace::{TraceCancelledPayload, TracerouteSetting};
use crate::probe::packet::match_udp_reply;
use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
use crate::socket::udp::{AsyncUdpSocket, UdpConfig};
use crate::socket::SocketFamily;
use anyhow::Result;
use bytes::Bytes;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

/// Default base target UDP port for traceroute
const DEFAULT_BASE_TARGET_UDP_PORT: u16 = 33435;

pub async fn udp_traceroute(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: &TracerouteSetting,
    token: CancellationToken,
) -> Result<bool> {
    let dst_ip = setting.ip_addr;
    let timeout = Duration::from_millis(setting.timeout_ms);

    // ICMP socket to receive Port Unreachable messages
    let icmp_kind = if dst_ip.is_ipv4() {
        IcmpKind::V4
    } else {
        IcmpKind::V6
    };
    let config = IcmpConfig::new(icmp_kind);
    // Linux ping sockets only deliver replies to their own echo requests.
    #[cfg(target_os = "linux")]
    let config = config.with_sock_type(crate::socket::icmp::IcmpSocketType::Raw);
    let icmp = AsyncIcmpSocket::new(&config).await?;

    let mut reached = false;

    'ttl_loop: for ttl in 1..=setting.max_hops {
        use crate::model::trace::TraceHop;

        if token.is_cancelled() {
            let _ = app.emit_logged(
                "traceroute:cancelled",
                TraceCancelledPayload {
                    run_id: run_id.to_string(),
                },
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
            run_id: run_id.to_string(),
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
            let res = tokio::time::timeout(timeout, async {
                loop {
                    let (n, from) = icmp.recv_from(&mut buf).await?;
                    if let Some(reached) =
                        match_udp_reply(target, local_addr.port(), from.ip(), &buf[..n])
                    {
                        return Ok::<_, std::io::Error>((from, reached));
                    }
                }
            })
            .await;

            match res {
                Err(_) => {
                    // timeout -> continue with remaining tries_per_hop
                    continue;
                }
                Ok(Err(e)) => {
                    best.note = Some(format!("recv error: {e}"));
                    break;
                }
                Ok(Ok((from, is_dest))) => {
                    let rtt = sent_at.elapsed().as_millis() as u64;
                    let from_ip = from.ip();

                    if best.rtt_ms.is_none_or(|cur| rtt < cur) {
                        best.rtt_ms = Some(rtt);
                        best.ip_addr = Some(from_ip);
                        best.note = None;
                    }

                    if is_dest {
                        best.reached = true;
                        reached = true;
                        app.emit_logged("traceroute:progress", &best).ok();
                        break 'ttl_loop;
                    }
                }
            }
        }

        if best.ip_addr.is_none() && best.note.is_none() {
            best.note = Some("timeout".into());
        }

        app.emit_logged("traceroute:progress", &best).ok();

        if token.is_cancelled() {
            let _ = app.emit_logged(
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
