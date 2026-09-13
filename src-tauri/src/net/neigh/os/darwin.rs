#![allow(non_camel_case_types)]

use libc::{c_int, c_uchar, pid_t, size_t};
use std::{
    collections::HashMap,
    ffi::c_void,
    io, mem,
    net::{IpAddr, Ipv4Addr},
    ptr,
};

use netdev::MacAddr;

const CTL_NET: c_int = 4;
#[allow(dead_code)]
const AF_ROUTE: c_int = 17;
const PF_ROUTE: c_int = 17;
const AF_LINK: c_int = 18;
const AF_INET: c_int = 2;

//const NET_RT_DUMP: c_int = 1;
const NET_RT_FLAGS: c_int = 2;

const RTM_VERSION: c_uchar = 5;
const RTF_LLINFO: c_int = 1024;

// sockaddr alignment
const SA_ALIGN: usize = 4;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct rt_metrics {
    rmx_locks: u32,
    rmx_mtu: u32,
    rmx_hopcount: u32,
    rmx_expire: i32,
    rmx_recvpipe: u32,
    rmx_sendpipe: u32,
    rmx_ssthresh: u32,
    rmx_rtt: u32,
    rmx_rttvar: u32,
    rmx_pksent: u32,
    rmx_state: u32,
    rmx_filler: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct rt_msghdr {
    rtm_msglen: u16,
    rtm_version: u8,
    rtm_type: u8,
    rtm_index: u16,
    rtm_flags: c_int,
    rtm_addrs: c_int,
    rtm_pid: pid_t,
    rtm_seq: c_int,
    rtm_errno: c_int,
    rtm_use: c_int,
    rtm_inits: u32,
    rtm_rmx: rt_metrics,
}

unsafe extern "C" {
    fn sysctl(
        name: *mut c_int,
        namelen: u32,
        oldp: *mut c_void,
        oldlenp: *mut size_t,
        newp: *mut c_void,
        newlen: size_t,
    ) -> c_int;
}

/// Fetches a sysctl value into a Vec<u8>.
fn sysctl_vec(mib: &mut [c_int]) -> io::Result<Vec<u8>> {
    let mut len: size_t = 0;
    let mut r = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            ptr::null_mut(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }

    let mut buf = vec![0u8; len as usize];
    r = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if r < 0 {
        // If the value grew, kernel returns ENOMEM. Retry once.
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ENOMEM) {
            let mut len2: size_t = 0;
            let r2 = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    ptr::null_mut(),
                    &mut len2,
                    ptr::null_mut(),
                    0,
                )
            };
            if r2 < 0 {
                return Err(io::Error::last_os_error());
            }
            buf.resize(len2 as usize, 0);
            let r3 = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    buf.as_mut_ptr() as *mut c_void,
                    &mut len2,
                    ptr::null_mut(),
                    0,
                )
            };
            if r3 < 0 {
                return Err(io::Error::last_os_error());
            }
            buf.truncate(len2 as usize);
            return Ok(buf);
        }
        return Err(err);
    }
    buf.truncate(len as usize);
    Ok(buf)
}

#[inline]
fn roundup(len: usize) -> usize {
    if len == 0 {
        SA_ALIGN
    } else {
        (len + (SA_ALIGN - 1)) & !(SA_ALIGN - 1)
    }
}

fn code_to_error(err: i32) -> io::Error {
    let kind = match err {
        17 => io::ErrorKind::AlreadyExists, // EEXIST
        3 => io::ErrorKind::NotFound,       // ESRCH
        3436 => io::ErrorKind::OutOfMemory, // ENOBUFS
        _ => io::ErrorKind::Other,
    };

    io::Error::new(kind, format!("rtm_errno {}", err))
}

/// Extract `(IP, MAC)` pair from a routing message's address block.
fn message_to_arppair(msg: &[u8]) -> Option<(IpAddr, MacAddr)> {
    let mut off = 0usize;
    let mut ip = None;
    let mut mac = None;
    while off + 2 <= msg.len() {
        let len = usize::from(msg[off]);
        if len == 0 {
            off += roundup(0);
            continue;
        }
        if len < 2 {
            return None;
        }
        let record = msg.get(off..off.checked_add(len)?)?;
        match c_int::from(record[1]) {
            AF_INET => {
                let start = mem::offset_of!(libc::sockaddr_in, sin_addr);
                let octets: [u8; 4] = record.get(start..start + 4)?.try_into().ok()?;
                ip = Some(IpAddr::V4(Ipv4Addr::from(octets)));
            }
            AF_LINK => {
                let nlen = usize::from(*record.get(mem::offset_of!(libc::sockaddr_dl, sdl_nlen))?);
                let alen = usize::from(*record.get(mem::offset_of!(libc::sockaddr_dl, sdl_alen))?);
                if alen >= 6 {
                    let start = mem::offset_of!(libc::sockaddr_dl, sdl_data) + nlen;
                    let address = record.get(start..start + alen)?;
                    mac = Some(MacAddr::from_octets(address[..6].try_into().ok()?));
                }
            }
            _ => {}
        }
        if let (Some(ip), Some(mac)) = (ip, mac) {
            return Some((ip, mac));
        }
        off += roundup(len);
    }
    None
}

fn parse_neighbor_table(buf: &[u8]) -> io::Result<HashMap<IpAddr, MacAddr>> {
    let mut arp_map = HashMap::new();
    let mut off = 0usize;
    while off < buf.len() {
        let header_len = mem::size_of::<rt_msghdr>();
        if buf.len() - off < header_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated routing header",
            ));
        }
        // The byte buffer does not guarantee alignment. The length check above
        // makes an unaligned copy of this integer-only C header safe.
        let hdr = unsafe { ptr::read_unaligned(buf[off..].as_ptr().cast::<rt_msghdr>()) };
        let msglen = usize::from(hdr.rtm_msglen);
        if msglen < header_len || msglen > buf.len() - off {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid routing message length",
            ));
        }
        if hdr.rtm_version == RTM_VERSION {
            if hdr.rtm_errno != 0 {
                return Err(code_to_error(hdr.rtm_errno));
            }
            if let Some((ip, mac)) = message_to_arppair(&buf[off + header_len..off + msglen]) {
                arp_map.insert(ip, mac);
            }
        }
        off += msglen;
    }
    Ok(arp_map)
}

/// Build an ARP/Neighbor table from the BSD/Darwin routing socket via `sysctl`.
pub fn get_neighbor_table() -> io::Result<HashMap<IpAddr, MacAddr>> {
    // sysctl net.route dump for ARP/neighbor entries (IPv4 only here).
    let mut mib = [
        CTL_NET,      // net
        PF_ROUTE,     // route
        0,            // 0
        AF_INET,      // IPv4
        NET_RT_FLAGS, // flags
        RTF_LLINFO,   // ARP/neighbor entries
    ];
    // Includes ENOMEM retry internally; length is truncated to actual bytes read.
    let buf = sysctl_vec(&mut mib)?;

    parse_neighbor_table(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_routing_records_and_headers() {
        assert!(parse_neighbor_table(&[0; 3]).is_err());
        let mut record = vec![0; mem::size_of::<rt_msghdr>()];
        record[..2].copy_from_slice(&1u16.to_ne_bytes());
        assert!(parse_neighbor_table(&record).is_err());
    }

    #[test]
    fn parses_unaligned_records_and_rejects_truncated_addresses() {
        let mut addresses = vec![0; 16 + 16];
        addresses[0] = 16;
        addresses[1] = AF_INET as u8;
        addresses[4..8].copy_from_slice(&[192, 0, 2, 1]);
        addresses[16] = 16;
        addresses[17] = AF_LINK as u8;
        addresses[22] = 6;
        addresses[24..30].copy_from_slice(&[0, 1, 2, 3, 4, 5]);
        let expected = message_to_arppair(&addresses).unwrap();
        assert_eq!(expected.0, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)));
        for end in 0..30 {
            assert!(message_to_arppair(&addresses[..end]).is_none());
        }
        let header_len = mem::size_of::<rt_msghdr>();
        let mut bytes = vec![0; 1 + header_len];
        bytes[1..3].copy_from_slice(&((header_len + addresses.len()) as u16).to_ne_bytes());
        bytes[3] = RTM_VERSION;
        bytes.extend_from_slice(&addresses);
        assert_eq!(
            parse_neighbor_table(&bytes[1..]).unwrap().get(&expected.0),
            Some(&expected.1)
        );
    }
}
