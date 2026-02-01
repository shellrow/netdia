use anyhow::Result;
use futures::{stream, StreamExt};
use rand::{seq::SliceRandom, thread_rng, Rng};
use tokio_util::sync::CancellationToken;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex};

use crate::model::endpoint::Host;
use crate::model::scan::{
    HostScanCancelledPayload, HostScanProgress, HostScanProgressPayload,
    HostScanReport, HostScanSetting, HostState, HostScanStartPayload,
};
use crate::probe::packet::{build_icmp_echo_bytes, parse_icmp_echo_v4, parse_icmp_echo_v6};
use crate::probe::scan::progress::ThrottledProgress;
use crate::probe::scan::tuner::hosts_concurrency;
use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
use crate::socket::SocketFamily;

struct Pending {
    #[allow(dead_code)]
    ip: IpAddr,
    sent_at: Instant,
    tx: oneshot::Sender<u64>,
}

fn spawn_receiver(
    socket: Arc<AsyncIcmpSocket>,
    pending: Arc<Mutex<HashMap<IpAddr, Pending>>>,
    is_v6: bool,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 2048];
        loop {
            let Ok((n, addr)) = socket.recv_from(&mut buf).await else {
                // Error on recv, socket might be closed
                break;
            };
            let is_echo_reply = if !is_v6 {
                // IPv4
                parse_icmp_echo_v4(&buf[..n]).is_some()
            } else {
                // IPv6
                parse_icmp_echo_v6(&buf[..n]).is_some()
            };

            if is_echo_reply {
                let mut map = pending.lock().await;
                if let Some(p) = map.remove(&addr.ip()) {
                    let _ = p.tx.send(p.sent_at.elapsed().as_millis() as u64);
                }
            }
        }
    })
}

pub async fn host_scan(
    app: &AppHandle,
    run_id: &str,
    src_ipv4: Option<IpAddr>,
    src_ipv6: Option<IpAddr>,
    mut setting: HostScanSetting,
    token: CancellationToken,
) -> Result<HostScanReport> {
    let app = app.clone();
    let run_id = run_id.to_string();

    let _ = app.emit("hostscan:start", HostScanStartPayload { run_id: run_id.clone() });

    let timeout = Duration::from_millis(setting.timeout_ms);
    let payload = setting.payload.clone().unwrap_or_else(|| "netd".to_string());
    let concurrency = setting.concurrency.unwrap_or(hosts_concurrency());
    if !setting.ordered {
        setting.targets.shuffle(&mut thread_rng());
    }

    // resolve
    let target_hosts: Vec<Host> = setting.resolve_targets().await;
    let target_map: HashMap<IpAddr, Host> = target_hosts.iter().map(|h| (h.ip, h.clone())).collect();
    let total = target_map.len() as u32;

    if total == 0 {
        let report = HostScanReport { run_id: run_id.clone(), alive: vec![], unreachable: vec![], total };
        let _ = app.emit("hostscan:done", report.clone());
        return Ok(report);
    }

    let progress = Arc::new(ThrottledProgress::new(total));

    // sockets
    let socket_v4 = if target_map.keys().any(|ip| ip.is_ipv4()) {
        let mut cfg = IcmpConfig::new(IcmpKind::V4);
        cfg = cfg.with_ttl(setting.hop_limit.max(1) as u32);
        Some(Arc::new(AsyncIcmpSocket::new(&cfg).await?))
    } else {
        None
    };

    let socket_v6 = if target_map.keys().any(|ip| ip.is_ipv6()) {
        let mut cfg = IcmpConfig::new(IcmpKind::V6);
        cfg = cfg.with_hoplimit(setting.hop_limit.max(1) as u32);
        Some(Arc::new(AsyncIcmpSocket::new(&cfg).await?))
    } else {
        None
    };

    let pending_v4: Arc<Mutex<HashMap<IpAddr, Pending>>> = Arc::new(Mutex::new(HashMap::new()));
    let pending_v6: Arc<Mutex<HashMap<IpAddr, Pending>>> = Arc::new(Mutex::new(HashMap::new()));

    let rx_v4 = socket_v4.as_ref().map(|s| spawn_receiver(s.clone(), pending_v4.clone(), false));
    let rx_v6 = socket_v6.as_ref().map(|s| spawn_receiver(s.clone(), pending_v6.clone(), true));

    let ip_list: Vec<IpAddr> = target_map.keys().cloned().collect();

    let socket_v4_for_tasks = socket_v4.clone();
    let socket_v6_for_tasks = socket_v6.clone();

    let mut tasks = stream::iter(ip_list.into_iter())
        .map(|dst_ip| {
            let app = app.clone();
            let run_id = run_id.clone();
            let token = token.clone();

            let socket_v4 = socket_v4_for_tasks.clone();
            let socket_v6 = socket_v6_for_tasks.clone();
            let pending_v4 = pending_v4.clone();
            let pending_v6 = pending_v6.clone();

            let progress = progress.clone();
            let payload = payload.clone();
            let timeout = timeout;
            let cnt = setting.count.max(1);
            let total = total;
            let src_ipv4 = src_ipv4;
            let src_ipv6 = src_ipv6;

            async move {
                if token.is_cancelled() {
                    return None;
                }

                let (sock_opt, pending_map, src_ip) = match SocketFamily::from_ip(&dst_ip) {
                    SocketFamily::IPV4 => (
                        socket_v4.clone(),
                        pending_v4.clone(),
                        src_ipv4.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
                    ),
                    SocketFamily::IPV6 => (
                        socket_v6.clone(),
                        pending_v6.clone(),
                        src_ipv6.unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
                    ),
                };

                let (state, rtt_ms, message) = if let Some(sock) = sock_opt {
                    let target = SocketAddr::new(dst_ip, 0);

                    let mut best_rtt: Option<u64> = None;
                    let mut last_err: Option<String> = None;

                    for seq in 1..=cnt {
                        if token.is_cancelled() {
                            let mut map = pending_map.lock().await;
                            map.remove(&dst_ip);
                            return None;
                        }

                        let id: u16 = rand::thread_rng().gen();
                        let (tx, rx) = oneshot::channel::<u64>();

                        {
                            let mut map = pending_map.lock().await;
                            map.insert(
                                dst_ip,
                                Pending {
                                    ip: dst_ip,
                                    sent_at: Instant::now(),
                                    tx,
                                },
                            );
                        }

                        let pkt = build_icmp_echo_bytes(
                            src_ip,
                            dst_ip,
                            id,
                            seq as u16,
                            payload.as_bytes(),
                        );

                        let send_res = tokio::select! {
                            _ = token.cancelled() => {
                                let mut map = pending_map.lock().await;
                                map.remove(&dst_ip);
                                return None;
                            }
                            r = sock.send_to(&pkt, target) => r,
                        };

                        if let Err(e) = send_res {
                            let mut map = pending_map.lock().await;
                            map.remove(&dst_ip);
                            last_err = Some(format!("send error: {}", e));
                            continue;
                        }
                        
                        let wait_res = tokio::select! {
                            _ = token.cancelled() => {
                                let mut map = pending_map.lock().await;
                                map.remove(&dst_ip);
                                return None;
                            }
                            r = tokio::time::timeout(timeout, rx) => r,
                        };

                        match wait_res {
                            Ok(Ok(rtt)) => {
                                best_rtt = Some(best_rtt.map_or(rtt, |b| b.min(rtt)));
                                break;
                            }
                            Ok(Err(_canceled)) => {
                                last_err = Some("wait canceled".into());
                            }
                            Err(_to) => {
                                let mut map = pending_map.lock().await;
                                map.remove(&dst_ip);
                                last_err = Some(format!("timeout (>{}ms)", timeout.as_millis()));
                            }
                        }
                    }

                    if let Some(rtt) = best_rtt {
                        (HostState::Alive, Some(rtt), None)
                    } else {
                        (HostState::Unreachable, None, last_err)
                    }
                } else {
                    (
                        HostState::Unreachable,
                        None,
                        Some("no suitable socket for IP family".into()),
                    )
                };

                let (done, should_emit) = progress.on_advance();

                let sample = HostScanProgress {
                    run_id: run_id.clone(),
                    ip_addr: dst_ip,
                    state,
                    rtt_ms,
                    message,
                    done,
                    total,
                };

                if matches!(sample.state, HostState::Alive) {
                    let _ = app.emit("hostscan:alive", sample.clone());
                }

                if should_emit {
                    let _ = app.emit(
                        "hostscan:progress",
                        HostScanProgressPayload {
                            run_id: run_id.clone(),
                            done,
                            total,
                        },
                    );
                }

                Some(sample)
            }
        })
        .buffer_unordered(concurrency);

    // collect
    let mut alive: Vec<(Host, u64)> = Vec::new();
    let mut unreachable: Vec<Host> = Vec::new();
    let mut cancelled = false;

    loop {
        let item = tokio::select! {
            _ = token.cancelled() => {
                cancelled = true;
                break;
            }
            s = tasks.next() => s,
        };

        let Some(item) = item else { break; };

        if let Some(p) = item {
            match p.state {
                HostState::Alive => {
                    if let Some(host) = target_map.get(&p.ip_addr) {
                        alive.push((host.clone(), p.rtt_ms.unwrap_or(0)));
                    }
                }
                HostState::Unreachable => {
                    if let Some(host) = target_map.get(&p.ip_addr) {
                        unreachable.push(host.clone());
                    }
                }
            }
        }
    }

    drop(tasks);

    // shutdown receivers
    drop(socket_v4_for_tasks);
    drop(socket_v6_for_tasks);
    drop(socket_v4);
    drop(socket_v6);
    if let Some(h) = rx_v4 { let _ = h.abort(); }
    if let Some(h) = rx_v6 { let _ = h.abort(); }

    if cancelled || token.is_cancelled() {
        let _ = app.emit("hostscan:cancelled", HostScanCancelledPayload { run_id: run_id.clone() });
        return Err(anyhow::anyhow!("cancelled"));
    }

    let report = HostScanReport {
        run_id: run_id.clone(),
        alive,
        unreachable,
        total,
    };

    let _ = app.emit("hostscan:done", report.clone());
    Ok(report)
}
