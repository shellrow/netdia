use netsock::{
    family::AddressFamilyFlags, get_sockets, protocol::ProtocolFlags, socket::SocketInfo,
};

/// Returns all sockets (IPv4/IPv6 x TCP/UDP) available on the host.
#[tauri::command]
pub fn get_sockets_all() -> Result<Vec<SocketInfo>, String> {
    // Combine IPv4 + IPv6 address families.
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;

    // Combine TCP + UDP protocol types.
    let pf = ProtocolFlags::TCP | ProtocolFlags::UDP;

    // Retrieve all sockets using netsock.
    let sockets = get_sockets(af, pf).map_err(|e| e.to_string())?;

    // Directly return the result as JSON since SocketInfo implements Serialize.
    Ok(sockets)
}
