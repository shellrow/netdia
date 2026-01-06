use netdev::MacAddr;
use netlink_packet_core::{NetlinkMessage, NetlinkPayload, NLM_F_DUMP, NLM_F_REQUEST};
use netlink_packet_route::{
    neighbour::{NeighbourAddress, NeighbourAttribute, NeighbourMessage},
    RouteNetlinkMessage,
};
use netlink_sys::{protocols::NETLINK_ROUTE, Socket, SocketAddr};
use std::{
    collections::HashMap,
    io::{self, ErrorKind},
    net::{IpAddr, Ipv4Addr},
    thread,
    time::{Duration, Instant},
};

const SEQ_BASE: u32 = 0x6E_65_74_64; // netd (netdia)
const RECV_BUFSZ: usize = 1 << 20; // 1MB
const RECV_TIMEOUT: Duration = Duration::from_secs(2);
const NLMSG_ALIGNTO: usize = 4;
const MIN_NLMSG_HEADER_LEN: usize = 16;

#[inline]
fn nlmsg_align(n: usize) -> usize {
    (n + NLMSG_ALIGNTO - 1) & !(NLMSG_ALIGNTO - 1)
}

fn open_route_socket() -> io::Result<Socket> {
    let mut sock = Socket::new(NETLINK_ROUTE)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("netlink open: {e}")))?;
    sock.bind_auto()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("bind_auto: {e}")))?;
    sock.set_non_blocking(true).ok();
    Ok(sock)
}

fn send_dump(sock: &mut Socket, msg: RouteNetlinkMessage, seq: u32) -> io::Result<()> {
    let mut nl = NetlinkMessage::from(msg);
    nl.header.flags = NLM_F_REQUEST | NLM_F_DUMP;
    nl.header.sequence_number = seq;
    nl.header.port_number = 0;
    nl.finalize();

    let blen = nl.buffer_len();
    if blen < MIN_NLMSG_HEADER_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("netlink message too short: buffer_len={}", blen),
        ));
    }

    let mut buf = vec![0; blen];
    nl.serialize(&mut buf);

    let kernel = SocketAddr::new(0, 0);
    sock.send_to(&buf, &kernel, 0)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("netlink send: {e}")))?;
    Ok(())
}

fn recv_multi(
    sock: &mut Socket,
    expect_seq: u32,
) -> io::Result<Vec<NetlinkMessage<RouteNetlinkMessage>>> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; RECV_BUFSZ];
    let kernel = SocketAddr::new(0, 0);
    let deadline = Instant::now() + RECV_TIMEOUT;

    loop {
        match sock.recv_from(&mut &mut buf[..], 0) {
            Ok((size, from)) => {
                let _ = from == kernel;
                let mut offset = 0usize;

                while offset < size {
                    if size - offset < MIN_NLMSG_HEADER_LEN {
                        break;
                    }
                    let bytes = &buf[offset..size];

                    let msg =
                        NetlinkMessage::<RouteNetlinkMessage>::deserialize(bytes).map_err(|e| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("deserialize: {e:?}"),
                            )
                        })?;

                    let consumed = msg.header.length as usize;
                    if consumed < MIN_NLMSG_HEADER_LEN || offset + consumed > size {
                        break;
                    }

                    if msg.header.sequence_number != expect_seq {
                        offset += nlmsg_align(consumed);
                        continue;
                    }

                    match &msg.payload {
                        NetlinkPayload::Done(_) => return Ok(out),
                        NetlinkPayload::Error(e) => {
                            if let Some(code) = e.code {
                                return Err(io::Error::new(
                                    io::ErrorKind::Other,
                                    format!("netlink error: code={}", code),
                                ));
                            }
                        }
                        NetlinkPayload::Noop | NetlinkPayload::Overrun(_) => { /* skip */ }
                        _ => out.push(msg),
                    }

                    offset += nlmsg_align(consumed);
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Ok(out);
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e),
        }
    }
}

fn dump_neigh() -> io::Result<Vec<NeighbourMessage>> {
    let mut sock = open_route_socket()?;
    let seq = SEQ_BASE ^ 0x04;
    send_dump(
        &mut sock,
        RouteNetlinkMessage::GetNeighbour(NeighbourMessage::default()),
        seq,
    )?;
    let msgs = recv_multi(&mut sock, seq)?;
    let mut out = Vec::new();
    for m in msgs {
        if let NetlinkPayload::InnerMessage(RouteNetlinkMessage::NewNeighbour(n)) = m.payload {
            out.push(n);
        }
    }
    Ok(out)
}

fn neigh_addr_to_ip(a: &NeighbourAddress) -> Option<IpAddr> {
    match a {
        NeighbourAddress::Inet(v4) => Some(IpAddr::V4(*v4)),
        NeighbourAddress::Inet6(v6) => Some(IpAddr::V6(*v6)),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn parse_mac_str(s: &str) -> Option<[u8; 6]> {
    // 00:11:22:33:44:55 / 00-11-22-33-44-55
    let cleaned = s.replace('-', ":");
    let parts: Vec<&str> = cleaned.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut b = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        if p.len() > 2 {
            return None;
        }
        b[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(b)
}

/// fallback: Read /proc/net/arp and build map
fn read_proc_net_arp() -> io::Result<HashMap<IpAddr, MacAddr>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let f = File::open("/proc/net/arp")?;
    let r = BufReader::new(f);
    let mut map = HashMap::new();

    // Header + lines:
    // IP address       HW type     Flags       HW address            Mask     Device
    for (i, line) in r.lines().enumerate() {
        let line = line?;
        if i == 0 {
            continue; // header
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let ip_s = cols[0];
        let flags_s = cols[2];
        let mac_s = cols[3];

        // Complete entries only (Flags = 0x2)
        if !flags_s.eq_ignore_ascii_case("0x2") {
            continue;
        }

        if let Ok(v4) = ip_s.parse::<Ipv4Addr>() {
            if let Some(raw) = parse_mac_str(mac_s) {
                map.insert(IpAddr::V4(v4), MacAddr::from_octets(raw));
            }
        }
    }
    Ok(map)
}

/// Dump neighbour(ARP/NDP) table via netlink.
/// If fails, fallback to read /proc/net/arp
pub fn get_neighbor_table() -> io::Result<HashMap<IpAddr, MacAddr>> {
    // netlink
    if let Ok(neighs) = dump_neigh() {
        if let Ok(m) = neighs_to_map(neighs) {
            if !m.is_empty() {
                return Ok(m);
            }
        }
    }
    // fallback: /proc/net/arp
    if let Ok(m) = read_proc_net_arp() {
        if !m.is_empty() {
            return Ok(m);
        }
    }
    Ok(HashMap::new())
}

fn neighs_to_map(neighs: Vec<NeighbourMessage>) -> io::Result<HashMap<IpAddr, MacAddr>> {
    let mut map = HashMap::new();

    for n in neighs {
        let mut ip: Option<IpAddr> = None;
        let mut mac: Option<[u8; 6]> = None;

        for nla in &n.attributes {
            match nla {
                NeighbourAttribute::Destination(a) => {
                    ip = neigh_addr_to_ip(a);
                }
                NeighbourAttribute::LinkLocalAddress(bytes) => {
                    if bytes.len() == 6 {
                        mac = Some([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]]);
                    }
                }
                _ => {}
            }
        }

        if let (Some(ip), Some(mac6)) = (ip, mac) {
            map.insert(ip, MacAddr::from_octets(mac6));
        }
    }

    Ok(map)
}
