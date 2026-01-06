use std::{collections::HashMap, io, net::IpAddr};

use netdev::MacAddr;

mod os;

pub fn get_neighbor_table() -> io::Result<HashMap<IpAddr, MacAddr>> {
    os::get_neighbor_table()
}
