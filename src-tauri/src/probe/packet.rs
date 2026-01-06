use bytes::Bytes;
use nex_packet::icmp::echo_reply::EchoReplyPacket as IcmpEchoReplyPacket;
use nex_packet::icmpv6::echo_reply::EchoReplyPacket as Icmpv6EchoReplyPacket;
use nex_packet::{
    builder::{icmp::IcmpPacketBuilder, icmpv6::Icmpv6PacketBuilder},
    icmp::{self, IcmpPacket, IcmpType},
    icmpv6::{self, Icmpv6Packet, Icmpv6Type},
    ipv4::Ipv4Packet,
    packet::Packet,
};
use std::net::IpAddr;

pub fn build_icmp_echo_bytes(src: IpAddr, dst: IpAddr, id: u16, seq: u16, payload: &[u8]) -> Bytes {
    match (src, dst) {
        (IpAddr::V4(s), IpAddr::V4(d)) => IcmpPacketBuilder::new(s, d)
            .icmp_type(IcmpType::EchoRequest)
            .icmp_code(icmp::echo_request::IcmpCodes::NoCode)
            .echo_fields(id, seq)
            .payload(Bytes::copy_from_slice(payload))
            .build()
            .to_bytes(),
        (IpAddr::V6(s), IpAddr::V6(d)) => Icmpv6PacketBuilder::new(s, d)
            .icmpv6_type(Icmpv6Type::EchoRequest)
            .icmpv6_code(icmpv6::echo_request::Icmpv6Codes::NoCode)
            .echo_fields(id, seq)
            .payload(Bytes::copy_from_slice(payload))
            .build()
            .to_bytes(),
        _ => panic!("Source and destination IP version mismatch"),
    }
}

/// Extract id/seq of ICMP Echo Reply (IPv4)
pub fn parse_icmp_echo_v4(buf: &[u8]) -> Option<IcmpEchoReplyPacket> {
    if let Some(ipv4_packet) = Ipv4Packet::from_buf(&buf) {
        if ipv4_packet.header.next_level_protocol == nex_packet::ip::IpNextProtocol::Icmp {
            if let Some(icmp_packet) = IcmpPacket::from_bytes(ipv4_packet.payload()) {
                match icmp::echo_reply::EchoReplyPacket::try_from(icmp_packet) {
                    Ok(reply) => {
                        return Some(reply);
                    }
                    Err(_) => {
                        return None;
                    }
                }
            }
        }
    }
    None
}

/// Extract id/seq of ICMPv6 Echo Reply. (ICMPv6 Header only)
/// The IPv6 header is automatically cropped off when recvfrom() is used.
pub fn parse_icmp_echo_v6(buf: &[u8]) -> Option<Icmpv6EchoReplyPacket> {
    if let Some(icmpv6_packet) = Icmpv6Packet::from_buf(&buf) {
        match icmpv6_packet.header.icmpv6_type {
            Icmpv6Type::EchoReply => match Icmpv6EchoReplyPacket::from_buf(&buf) {
                Some(reply) => {
                    return Some(reply);
                }
                None => {
                    return Some(Icmpv6EchoReplyPacket {
                        header: icmpv6_packet.header,
                        identifier: 0,
                        sequence_number: 0,
                        payload: icmpv6_packet.payload,
                    });
                }
            },
            _ => {
                return Some(Icmpv6EchoReplyPacket {
                    header: icmpv6_packet.header,
                    identifier: 0,
                    sequence_number: 0,
                    payload: icmpv6_packet.payload,
                });
            }
        }
    }
    None
}
