use bytes::Bytes;
use nex_packet::icmp::echo_reply::EchoReplyPacket as IcmpEchoReplyPacket;
use nex_packet::icmpv6::echo_reply::EchoReplyPacket as Icmpv6EchoReplyPacket;
use nex_packet::{
    builder::{icmp::IcmpPacketBuilder, icmpv6::Icmpv6PacketBuilder},
    icmp::{self, IcmpPacket, IcmpType},
    icmpv6::{self, Icmpv6Type},
    ipv4::Ipv4Packet,
    packet::Packet,
};
use std::net::IpAddr;

pub fn build_icmp_echo_bytes(
    src: IpAddr,
    dst: IpAddr,
    id: u16,
    seq: u16,
    payload: &[u8],
) -> anyhow::Result<Bytes> {
    // Set payload first: echo_fields prepends id/sequence to the builder payload,
    // while a later payload call replaces them along with the existing data.
    match (src, dst) {
        (IpAddr::V4(s), IpAddr::V4(d)) => Ok(IcmpPacketBuilder::new(s, d)
            .icmp_type(IcmpType::EchoRequest)
            .icmp_code(icmp::echo_request::IcmpCodes::NoCode)
            .payload(Bytes::copy_from_slice(payload))
            .echo_fields(id, seq)
            .build()?
            .to_bytes()),
        (IpAddr::V6(s), IpAddr::V6(d)) => Ok(Icmpv6PacketBuilder::new(s, d)
            .icmpv6_type(Icmpv6Type::EchoRequest)
            .icmpv6_code(icmpv6::echo_request::Icmpv6Codes::NoCode)
            .payload(Bytes::copy_from_slice(payload))
            .echo_fields(id, seq)
            .build()?
            .to_bytes()),
        _ => anyhow::bail!("source and destination IP versions do not match"),
    }
}

/// Extract id/seq of ICMP Echo Reply (IPv4)
pub fn parse_icmp_echo_v4(buf: &[u8]) -> Option<IcmpEchoReplyPacket> {
    let outer;
    let payload;
    let bytes = if buf.first().is_some_and(|byte| byte >> 4 == 4) {
        outer = Ipv4Packet::try_from_buf(buf).ok()?;
        if outer.header.next_level_protocol != nex_packet::ip::IpNextProtocol::Icmp {
            return None;
        }
        payload = outer.payload();
        payload.as_ref()
    } else {
        // Linux datagram ICMP sockets return the ICMP header without IPv4.
        buf
    };
    if bytes.first() != Some(&0) || bytes.get(1) != Some(&0) {
        return None;
    }
    IcmpEchoReplyPacket::try_from(IcmpPacket::try_from_buf(bytes).ok()?).ok()
}

/// Extract id/seq of ICMPv6 Echo Reply. (ICMPv6 Header only)
/// The IPv6 header is automatically cropped off when recvfrom() is used.
pub fn parse_icmp_echo_v6(buf: &[u8]) -> Option<Icmpv6EchoReplyPacket> {
    let reply = Icmpv6EchoReplyPacket::try_from_buf(buf).ok()?;
    if reply.header.icmpv6_type == Icmpv6Type::EchoReply && buf.get(1) == Some(&0) {
        Some(reply)
    } else {
        None
    }
}

/// Match a reply to the exact transmitted echo request.
pub fn matches_echo_reply(dst: IpAddr, from: IpAddr, buf: &[u8], id: u16, seq: u16) -> bool {
    if from != dst {
        return false;
    }
    let fields = if dst.is_ipv4() {
        parse_icmp_echo_v4(buf).map(|reply| (reply.identifier, reply.sequence_number))
    } else {
        parse_icmp_echo_v6(buf).map(|reply| (reply.identifier, reply.sequence_number))
    };
    fields == Some((id, seq))
}

/// Return whether the destination was reached, or None for unrelated traffic.
pub fn match_trace_reply(dst: IpAddr, from: IpAddr, buf: &[u8], id: u16, seq: u16) -> Option<bool> {
    if matches_echo_reply(dst, from, buf, id, seq) {
        return Some(true);
    }
    let outer;
    let payload;
    let icmp = if dst.is_ipv4() {
        outer = Ipv4Packet::try_from_buf(buf).ok()?;
        if outer.header.next_level_protocol != nex_packet::ip::IpNextProtocol::Icmp {
            return None;
        }
        payload = outer.payload();
        payload.as_ref()
    } else {
        buf
    };
    let time_exceeded = if dst.is_ipv4() { 11 } else { 3 };
    if icmp.first() != Some(&time_exceeded) || icmp.get(1) != Some(&0) {
        return None;
    }
    let quoted = icmp.get(8..)?;
    let request = match dst {
        IpAddr::V4(ip) => {
            let header_len = usize::from(*quoted.first()? & 0x0f) * 4;
            if quoted[0] >> 4 != 4
                || header_len < 20
                || quoted.get(9) != Some(&1)
                || quoted.get(16..20)? != ip.octets()
            {
                return None;
            }
            quoted.get(header_len..)?
        }
        IpAddr::V6(ip) => {
            // Extension headers are not generated for these echo requests.
            if quoted.first()? >> 4 != 6
                || quoted.get(6) != Some(&58)
                || quoted.get(24..40)? != ip.octets()
            {
                return None;
            }
            quoted.get(40..)?
        }
    };
    let expected_type = if dst.is_ipv4() { 8 } else { 128 };
    if request.first() != Some(&expected_type)
        || request.get(1) != Some(&0)
        || request.get(4..6)? != id.to_be_bytes()
        || request.get(6..8)? != seq.to_be_bytes()
    {
        return None;
    }
    Some(false)
}

/// Match an ICMP error to the quoted UDP destination and both ports.
/// Returns true only for a port-unreachable reply from the destination itself.
#[cfg(any(unix, test))]
pub fn match_udp_reply(
    dst: std::net::SocketAddr,
    source_port: u16,
    from: IpAddr,
    buf: &[u8],
) -> Option<bool> {
    let outer;
    let payload;
    let icmp = if dst.is_ipv4() {
        outer = Ipv4Packet::try_from_buf(buf).ok()?;
        if outer.header.next_level_protocol != nex_packet::ip::IpNextProtocol::Icmp {
            return None;
        }
        payload = outer.payload();
        payload.as_ref()
    } else {
        buf
    };
    let reached = match (dst.is_ipv4(), *icmp.first()?, *icmp.get(1)?) {
        (true, 3, 3) | (false, 1, 4) if from == dst.ip() => true,
        (true, 11, 0) | (false, 3, 0) => false,
        _ => return None,
    };
    let quoted = icmp.get(8..)?;
    let udp = match dst.ip() {
        IpAddr::V4(ip) => {
            let header_len = usize::from(*quoted.first()? & 15) * 4;
            if quoted[0] >> 4 != 4
                || header_len < 20
                || quoted.get(9) != Some(&17)
                || quoted.get(16..20)? != ip.octets()
                || u16::from_be_bytes(quoted.get(6..8)?.try_into().ok()?) & 0x1fff != 0
            {
                return None;
            }
            quoted.get(header_len..)?
        }
        IpAddr::V6(ip) => {
            if quoted.first()? >> 4 != 6
                || quoted.get(6) != Some(&17)
                || quoted.get(24..40)? != ip.octets()
            {
                return None;
            }
            quoted.get(40..)?
        }
    };
    if udp.len() < 8
        || udp.get(..2)? != source_port.to_be_bytes()
        || udp.get(2..4)? != dst.port().to_be_bytes()
    {
        return None;
    }
    Some(reached)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_builder_preserves_fields_and_payload() {
        for address in ["127.0.0.1", "::1"] {
            let ip: IpAddr = address.parse().unwrap();
            for payload in [b"".as_slice(), b"a", b"netd", b"longer echo payload"] {
                let packet = build_icmp_echo_bytes(ip, ip, 0x1234, 0x5678, payload).unwrap();
                assert_eq!(packet.len(), 8 + payload.len());
                assert_eq!(packet[0], if ip.is_ipv4() { 8 } else { 128 });
                assert_eq!(packet[1], 0);
                assert_eq!(&packet[4..6], &0x1234u16.to_be_bytes());
                assert_eq!(&packet[6..8], &0x5678u16.to_be_bytes());
                assert_eq!(&packet[8..], payload);
                let mut checksum_input = Vec::new();
                if let IpAddr::V6(ip) = ip {
                    checksum_input.extend_from_slice(&ip.octets());
                    checksum_input.extend_from_slice(&ip.octets());
                    checksum_input.extend_from_slice(&(packet.len() as u32).to_be_bytes());
                    checksum_input.extend_from_slice(&[0, 0, 0, 58]);
                }
                checksum_input.extend_from_slice(&packet);
                let mut sum: u32 = checksum_input
                    .chunks(2)
                    .map(|word| {
                        u32::from(u16::from_be_bytes([word[0], *word.get(1).unwrap_or(&0)]))
                    })
                    .sum();
                while sum >> 16 != 0 {
                    sum = (sum & 0xffff) + (sum >> 16);
                }
                assert_eq!(sum, 0xffff, "invalid echo checksum for {address}");
            }
        }
    }

    async fn live_echo(address: &str) {
        use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
        use std::{net::SocketAddr, time::Duration};

        let ip: IpAddr = address.parse().unwrap();
        let kind = if ip.is_ipv4() {
            IcmpKind::V4
        } else {
            IcmpKind::V6
        };
        let socket = AsyncIcmpSocket::new(&IcmpConfig::new(kind)).await.unwrap();
        let id = socket.echo_identifier(0x1234).unwrap();
        for seq in 1..=3 {
            let packet = build_icmp_echo_bytes(ip, ip, id, seq, b"netd").unwrap();
            socket
                .send_to(&packet, SocketAddr::new(ip, 0))
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                let mut buffer = [0; 2048];
                loop {
                    let (len, from) = socket.recv_from(&mut buffer).await.unwrap();
                    if matches_echo_reply(ip, from.ip(), &buffer[..len], id, seq) {
                        assert_eq!(
                            match_trace_reply(ip, from.ip(), &buffer[..len], id, seq),
                            Some(true)
                        );
                        break;
                    }
                }
            })
            .await
            .expect("no matching echo reply from loopback");
        }
    }

    #[tokio::test]
    #[ignore = "requires local ICMP socket permission and IPv4 loopback"]
    async fn live_icmp_echo_v4() {
        live_echo("127.0.0.1").await;
    }

    #[tokio::test]
    #[ignore = "requires local ICMP socket permission and IPv6 loopback"]
    async fn live_icmp_echo_v6() {
        live_echo("::1").await;
    }

    fn v4_packet(payload: &[u8]) -> Vec<u8> {
        let mut packet = vec![0; 20];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&((20 + payload.len()) as u16).to_be_bytes());
        packet[9] = 1;
        packet[16..20].copy_from_slice(&[192, 0, 2, 1]);
        packet.extend_from_slice(payload);
        packet
    }

    #[test]
    fn echo_matching_rejects_unrelated_and_truncated_replies() {
        for dst in ["192.0.2.1", "2001:db8::1"] {
            let dst: IpAddr = dst.parse().unwrap();
            let mut reply = vec![if dst.is_ipv4() { 0 } else { 129 }, 0, 0, 0, 0, 42, 0, 7];
            if dst.is_ipv4() {
                reply = v4_packet(&reply);
            }
            assert!(matches_echo_reply(dst, dst, &reply, 42, 7));
            assert!(!matches_echo_reply(dst, dst, &reply, 43, 7));
            assert!(!matches_echo_reply(dst, dst, &reply, 42, 8));
            assert!(!matches_echo_reply(
                dst,
                "127.0.0.1".parse().unwrap(),
                &reply,
                42,
                7
            ));
            for end in 0..reply.len() {
                assert!(!matches_echo_reply(dst, dst, &reply[..end], 42, 7));
            }
        }
        assert!(parse_icmp_echo_v6(&[1, 0, 0, 0, 0, 42, 0, 7]).is_none());
        assert!(parse_icmp_echo_v6(&[128, 0, 0, 0, 0, 42, 0, 7]).is_none());
    }

    #[test]
    fn traceroute_matches_quoted_request() {
        let dst = "192.0.2.1".parse().unwrap();
        let router = "192.0.2.254".parse().unwrap();
        let request = v4_packet(&build_icmp_echo_bytes(router, dst, 42, 7, b"netd").unwrap());
        let mut exceeded = vec![11, 0, 0, 0, 0, 0, 0, 0];
        exceeded.extend_from_slice(&request);
        let reply = v4_packet(&exceeded);
        assert_eq!(match_trace_reply(dst, router, &reply, 42, 7), Some(false));
        assert_eq!(match_trace_reply(dst, router, &reply, 43, 7), None);
        assert_eq!(
            match_trace_reply("192.0.2.2".parse().unwrap(), router, &reply, 42, 7),
            None
        );
        // ICMP errors only need to quote the original IP header and eight bytes.
        for end in 0..reply.len() - 4 {
            assert_eq!(match_trace_reply(dst, router, &reply[..end], 42, 7), None);
        }
        assert_eq!(
            match_trace_reply(dst, router, &reply[..reply.len() - 4], 42, 7),
            Some(false)
        );
    }
}

#[cfg(test)]
mod udp_tests {
    use super::*;
    use std::net::SocketAddr;

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "requires ICMP error socket permission and dual-family loopback"]
    async fn live_udp_port_unreachable() {
        use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
        use std::time::Duration;

        for address in ["127.0.0.1:0", "[::1]:0"] {
            let reservation = std::net::UdpSocket::bind(address).unwrap();
            let target = reservation.local_addr().unwrap();
            let kind = if target.is_ipv4() {
                IcmpKind::V4
            } else {
                IcmpKind::V6
            };
            let config = IcmpConfig::new(kind);
            #[cfg(target_os = "linux")]
            let config = config.with_sock_type(crate::socket::icmp::IcmpSocketType::Raw);
            let icmp = AsyncIcmpSocket::new(&config).await.unwrap();
            let udp = tokio::net::UdpSocket::bind(address).await.unwrap();
            let source_port = udp.local_addr().unwrap().port();
            drop(reservation);
            udp.send_to(b"netd", target).await.unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                let mut buffer = [0; 2048];
                loop {
                    let (len, from) = icmp.recv_from(&mut buffer).await.unwrap();
                    if match_udp_reply(target, source_port, from.ip(), &buffer[..len]) == Some(true)
                    {
                        break;
                    }
                }
            })
            .await
            .expect("no matching UDP port-unreachable reply from loopback");
        }
    }

    fn reply(dst: SocketAddr, source: u16, kind: u8, code: u8) -> Vec<u8> {
        let mut icmp = vec![kind, code, 0, 0, 0, 0, 0, 0];
        let mut quoted = vec![0; if dst.is_ipv4() { 20 } else { 40 }];
        match dst.ip() {
            IpAddr::V4(ip) => {
                quoted[0] = 0x45;
                quoted[9] = 17;
                quoted[16..20].copy_from_slice(&ip.octets());
            }
            IpAddr::V6(ip) => {
                quoted[0] = 0x60;
                quoted[6] = 17;
                quoted[24..40].copy_from_slice(&ip.octets());
            }
        }
        quoted.extend_from_slice(&source.to_be_bytes());
        quoted.extend_from_slice(&dst.port().to_be_bytes());
        quoted.extend_from_slice(&[0, 12, 0, 0]);
        icmp.extend_from_slice(&quoted);
        if dst.is_ipv4() {
            let mut ip = vec![0; 20];
            ip[0] = 0x45;
            ip[9] = 1;
            let len = (20 + icmp.len()) as u16;
            ip[2..4].copy_from_slice(&len.to_be_bytes());
            ip.extend_from_slice(&icmp);
            ip
        } else {
            icmp
        }
    }

    #[test]
    fn udp_errors_require_matching_destination_ports_type_and_code() {
        for address in ["192.0.2.1:33435", "[2001:db8::1]:33435"] {
            let dst: SocketAddr = address.parse().unwrap();
            let (kind, code) = if dst.is_ipv4() { (3, 3) } else { (1, 4) };
            let bytes = reply(dst, 50000, kind, code);
            assert_eq!(match_udp_reply(dst, 50000, dst.ip(), &bytes), Some(true));
            assert_eq!(match_udp_reply(dst, 50001, dst.ip(), &bytes), None);
            let mut other = dst;
            other.set_port(33436);
            assert_eq!(match_udp_reply(other, 50000, dst.ip(), &bytes), None);
            let other_ip = if dst.is_ipv4() {
                "192.0.2.2"
            } else {
                "2001:db8::2"
            }
            .parse()
            .unwrap();
            assert_eq!(match_udp_reply(dst, 50000, other_ip, &bytes), None);
            other.set_ip(other_ip);
            other.set_port(dst.port());
            assert_eq!(match_udp_reply(other, 50000, other_ip, &bytes), None);
            for n in 0..bytes.len() {
                assert_eq!(match_udp_reply(dst, 50000, dst.ip(), &bytes[..n]), None);
            }
            assert_eq!(
                match_udp_reply(dst, 50000, dst.ip(), &reply(dst, 50000, kind, 0)),
                None
            );
            let exceeded = reply(dst, 50000, if dst.is_ipv4() { 11 } else { 3 }, 0);
            assert_eq!(
                match_udp_reply(dst, 50000, other_ip, &exceeded),
                Some(false)
            );
        }
    }
}
