pub mod dns;
pub mod interface;
pub mod internet;
pub mod latency;
pub mod neigh;
pub mod route;
pub mod speedtest;
pub mod sys;

pub(crate) fn http_client_builder() -> reqwest::ClientBuilder {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires a configured loopback interface and a populated OS routing table"]
    fn live_network_inventory() {
        let interfaces = super::interface::list_interfaces();
        assert!(interfaces
            .iter()
            .any(|iface| iface.ip_addrs().iter().any(std::net::IpAddr::is_loopback)));
        assert!(!super::route::list_routes().unwrap().is_empty());
        super::neigh::get_neighbor_table().unwrap();
    }
}
