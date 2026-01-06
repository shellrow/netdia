use netdev::MacAddr;
use std::{
    collections::HashMap,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    ptr,
};

use windows_sys::Win32::{
    Foundation::NO_ERROR,
    NetworkManagement::IpHelper::{FreeMibTable, GetIpNetTable2, MIB_IPNET_ROW2, MIB_IPNET_TABLE2},
    Networking::WinSock::{
        NlnsDelay, NlnsPermanent, NlnsProbe, NlnsReachable, NlnsStale, ADDRESS_FAMILY, AF_INET,
        AF_INET6, IN6_ADDR, IN_ADDR, SOCKADDR_INET,
    },
};

pub fn get_neighbor_table() -> io::Result<HashMap<IpAddr, MacAddr>> {
    let mut map = HashMap::new();
    // IPv4(ARP)
    if let Ok(m) = dump_ipnet(AF_INET) {
        map.extend(m);
    }
    // IPv6(NDP)
    if let Ok(m) = dump_ipnet(AF_INET6) {
        map.extend(m);
    }
    Ok(map)
}

fn dump_ipnet(af: ADDRESS_FAMILY) -> io::Result<HashMap<IpAddr, MacAddr>> {
    let mut out = HashMap::new();

    unsafe {
        let mut table_ptr: *mut MIB_IPNET_TABLE2 = ptr::null_mut();
        let ret = GetIpNetTable2(af, &mut table_ptr);
        if ret != NO_ERROR {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("GetIpNetTable2 failed: {ret}"),
            ));
        }
        if table_ptr.is_null() {
            return Ok(out);
        }

        // free on scope exit
        let table: &MIB_IPNET_TABLE2 = &*table_ptr;
        let rows: &[MIB_IPNET_ROW2] =
            std::slice::from_raw_parts(table.Table.as_ptr(), table.NumEntries as usize);

        for row in rows {
            if row.PhysicalAddressLength != 6 {
                continue;
            }

            if !is_interesting_state(row.State) {
                continue;
            }

            if let Some(ip) = sockaddr_inet_to_ip(&row.Address) {
                let mac = MacAddr::from_octets([
                    row.PhysicalAddress[0],
                    row.PhysicalAddress[1],
                    row.PhysicalAddress[2],
                    row.PhysicalAddress[3],
                    row.PhysicalAddress[4],
                    row.PhysicalAddress[5],
                ]);
                out.insert(ip, mac);
            }
        }

        FreeMibTable(table_ptr as _);
    }

    Ok(out)
}

#[allow(non_upper_case_globals)]
#[inline]
fn is_interesting_state(state: i32) -> bool {
    matches!(
        state,
        NlnsPermanent | NlnsReachable | NlnsStale | NlnsDelay | NlnsProbe
    )
}

#[inline]
fn sockaddr_inet_to_ip(sa: &SOCKADDR_INET) -> Option<IpAddr> {
    unsafe {
        match sa.si_family {
            AF_INET => {
                let IN_ADDR { S_un: s } = sa.Ipv4.sin_addr;
                let bytes = s.S_un_b;
                Some(IpAddr::V4(Ipv4Addr::new(
                    bytes.s_b1, bytes.s_b2, bytes.s_b3, bytes.s_b4,
                )))
            }
            AF_INET6 => {
                let IN6_ADDR { u: u6 } = sa.Ipv6.sin6_addr;
                let segs = u6.Byte;
                Some(IpAddr::V6(Ipv6Addr::from(segs)))
            }
            _ => None,
        }
    }
}
