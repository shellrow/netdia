use super::pool::ResolverPool;
use anyhow::{Context, Result};
use hickory_resolver::proto::rr::{
    rdata::{A, AAAA, CERT, MX, NS, SOA, SRV, TLSA, TXT},
    RecordData, RecordType,
};
use std::{
    net::{Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use crate::model::dns::{
    CertRecord, DomainLookupInfo, MxRecord, SoaRecord, SrvRecord, TlsaRecord, TxtRecord,
};

pub fn get_resolver() -> Result<ResolverPool> {
    ResolverPool::from_system().context("Failed to initialize system DNS resolvers")
}

#[derive(Clone)]
pub struct DnsResolver {
    inner: Arc<ResolverPool>,
}

impl DnsResolver {
    pub fn new() -> Result<Self> {
        let r = get_resolver()?;
        Ok(Self { inner: Arc::new(r) })
    }

    #[allow(unused)]
    pub fn from_resolver(inner: ResolverPool) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    #[inline]
    fn fqdn(name: &str) -> String {
        if name.ends_with('.') {
            name.to_owned()
        } else {
            format!("{name}.")
        }
    }

    pub async fn a(&self, name: &str) -> Vec<Ipv4Addr> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::A)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| A::try_borrow(&r.data))
                    .map(|a| a.0)
                    .collect::<Vec<Ipv4Addr>>()
            })
            .unwrap_or_default()
    }

    pub async fn aaaa(&self, name: &str) -> Vec<Ipv6Addr> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::AAAA)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| AAAA::try_borrow(&r.data))
                    .map(|aaaa| aaaa.0)
                    .collect::<Vec<Ipv6Addr>>()
            })
            .unwrap_or_default()
    }

    pub async fn mx(&self, name: &str) -> Vec<MxRecord> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::MX)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| MX::try_borrow(&r.data))
                    .map(|mx: &MX| MxRecord {
                        preference: mx.preference,
                        exchange: mx.exchange.to_utf8(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn ns(&self, name: &str) -> Vec<String> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::NS)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| NS::try_borrow(&r.data))
                    .map(|ns: &NS| ns.to_utf8())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn soa(&self, name: &str) -> Vec<SoaRecord> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::SOA)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| SOA::try_borrow(&r.data))
                    .map(|soa: &SOA| SoaRecord {
                        mname: soa.mname.to_utf8(),
                        rname: soa.rname.to_utf8(),
                        serial: soa.serial,
                        refresh: soa.refresh,
                        retry: soa.retry,
                        expire: soa.expire,
                        minimum: soa.minimum,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::SRV)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| SRV::try_borrow(&r.data))
                    .map(|srv: &SRV| SrvRecord {
                        priority: srv.priority,
                        weight: srv.weight,
                        port: srv.port,
                        target: srv.target.to_utf8(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn tlsa(&self, name: &str) -> Vec<TlsaRecord> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::TLSA)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| TLSA::try_borrow(&r.data))
                    .map(|t: &TLSA| TlsaRecord {
                        cert_usage: u8::from(t.cert_usage),
                        selector: u8::from(t.selector),
                        matching: u8::from(t.matching),
                        cert_data_base64: data_encoding::BASE64.encode(&t.cert_data),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn txt(&self, name: &str) -> Vec<TxtRecord> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::TXT)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| TXT::try_borrow(&r.data))
                    .flat_map(|txt: &TXT| {
                        let r = txt.to_string();
                        r.split_once('=')
                            .map(|(k, v)| TxtRecord {
                                key: k.to_string(),
                                value: v.to_string(),
                            })
                            .or_else(|| {
                                Some(TxtRecord {
                                    key: r.clone(),
                                    value: String::new(),
                                })
                            })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn cert(&self, name: &str) -> Vec<CertRecord> {
        let q = Self::fqdn(name);
        self.inner
            .lookup(&q, RecordType::CERT)
            .await
            .map(|l| {
                l.answers()
                    .iter()
                    .filter_map(|r| CERT::try_borrow(&r.data))
                    .map(|c: &CERT| CertRecord {
                        cert_type: u16::from(c.cert_type),
                        key_tag: c.key_tag,
                        algorithm: u8::from(c.algorithm),
                        cert_data_base64: c.cert_base64(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn lookup_all(&self, name: &str) -> Result<DomainLookupInfo> {
        let (a, aaaa, mx, ns, soa, srv, tlsa, txt, cert) = tokio::join!(
            self.a(name),
            self.aaaa(name),
            self.mx(name),
            self.ns(name),
            self.soa(name),
            self.srv(name),
            self.tlsa(name),
            self.txt(name),
            self.cert(name),
        );

        Ok(DomainLookupInfo {
            name: name.to_string(),
            a,
            aaaa,
            mx,
            ns,
            soa,
            srv,
            tlsa,
            txt,
            cert,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::{
        op::{Message, OpCode, ResponseCode},
        rr::{Name, RData, Record, RecordType},
    };
    use hickory_resolver::config::{
        ConnectionConfig, NameServerConfig, ResolverConfig, ResolverOpts,
    };
    use serde_json::json;
    use std::time::Duration;
    use tokio::net::UdpSocket;

    struct TestServer(tokio::task::JoinHandle<()>);

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    async fn resolver() -> (DnsResolver, TestServer) {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut buf = [0; 4096];
            loop {
                let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
                let request = Message::from_vec(&buf[..n]).unwrap();
                let query = request.queries[0].clone();
                let mut response = Message::response(request.metadata.id, OpCode::Query);
                response.metadata.authoritative = true;
                response.metadata.recursion_available = true;
                response.add_query(query.clone());
                if query.name().to_ascii() == "missing.test." {
                    response.metadata.response_code = ResponseCode::NXDomain;
                } else {
                    let value = match query.query_type() {
                        RecordType::A => "192.0.2.42",
                        RecordType::AAAA => "2001:db8::42",
                        RecordType::MX => "10 mail.example.test.",
                        RecordType::NS => "ns.example.test.",
                        RecordType::SOA => {
                            "ns.example.test. admin.example.test. 42 3600 600 86400 60"
                        }
                        RecordType::SRV => "1 2 443 service.example.test.",
                        RecordType::TLSA => "3 1 1 010203",
                        RecordType::TXT => "\"key=value=tail\" \"-suffix\"",
                        RecordType::CERT => "1 123 8 AQID",
                        RecordType::PTR => "host.example.test.",
                        other => panic!("unexpected query type: {other}"),
                    };
                    let target = if matches!(query.query_type(), RecordType::SOA | RecordType::SRV)
                    {
                        query.name().clone()
                    } else {
                        let target = Name::from_ascii("canonical.example.test.").unwrap();
                        response.add_answer(Record::from_rdata(
                            query.name().clone(),
                            60,
                            RData::CNAME(hickory_proto::rr::rdata::CNAME(target.clone())),
                        ));
                        target
                    };
                    response.add_answer(Record::from_rdata(
                        target,
                        60,
                        RData::try_from_str(query.query_type(), value).unwrap(),
                    ));
                    // Additional records must not leak into the displayed answer list.
                    response.add_additional(Record::from_rdata(
                        Name::from_ascii("unrelated.example.test.").unwrap(),
                        60,
                        RData::try_from_str(query.query_type(), value).unwrap(),
                    ));
                }
                socket
                    .send_to(&response.to_vec().unwrap(), peer)
                    .await
                    .unwrap();
            }
        });
        let mut connection = ConnectionConfig::udp();
        connection.port = addr.port();
        let config = ResolverConfig::from_name_servers(vec![NameServerConfig::new(
            addr.ip(),
            true,
            vec![connection],
        )]);
        let mut options = ResolverOpts::default();
        options.timeout = Duration::from_secs(1);
        options.attempts = 1;
        (
            DnsResolver::from_resolver(ResolverPool::from_config(config, options).unwrap()),
            TestServer(task),
        )
    }

    #[tokio::test]
    async fn all_record_types_preserve_the_frontend_payload_with_cname_answers() {
        let (resolver, _server) = resolver().await;
        let result =
            tokio::time::timeout(Duration::from_secs(5), resolver.lookup_all("example.test"))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({
                "name": "example.test",
                "a": ["192.0.2.42"], "aaaa": ["2001:db8::42"],
                "mx": [{"preference": 10, "exchange": "mail.example.test."}],
                "ns": ["ns.example.test."],
                "soa": [{"mname": "ns.example.test.", "rname": "admin.example.test.",
                    "serial": 42, "refresh": 3600, "retry": 600, "expire": 86400, "minimum": 60}],
                "srv": [{"priority": 1, "weight": 2, "port": 443, "target": "service.example.test."}],
                "tlsa": [{"cert_usage": 3, "selector": 1, "matching": 1, "cert_data_base64": "AQID"}],
                "txt": [{"key": "key", "value": "value=tail-suffix"}],
                "cert": [{"cert_type": 1, "key_tag": 123, "algorithm": 8, "cert_data_base64": "AQID"}]
            })
        );
        assert_eq!(
            resolver.a("example.test.").await,
            vec![Ipv4Addr::new(192, 0, 2, 42)]
        );
    }

    #[tokio::test]
    async fn missing_domain_preserves_empty_record_lists() {
        let (resolver, _server) = resolver().await;
        let result =
            tokio::time::timeout(Duration::from_secs(5), resolver.lookup_all("missing.test"))
                .await
                .unwrap()
                .unwrap();
        let result = serde_json::to_value(result).unwrap();
        for field in ["a", "aaaa", "mx", "ns", "soa", "srv", "tlsa", "txt", "cert"] {
            assert_eq!(result[field], json!([]), "{field}");
        }
    }

    #[tokio::test]
    async fn forward_and_reverse_lookups_handle_cname_answers() {
        let (resolver, _server) = resolver().await;
        let ips = resolver.inner.lookup_ip("example.test.").await.unwrap();
        assert!(ips
            .iter()
            .any(|ip| ip == "192.0.2.42".parse::<std::net::IpAddr>().unwrap()));
        let name = crate::net::dns::reverse_lookup_with_resolver(
            &resolver.inner,
            "192.0.2.42".parse().unwrap(),
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(name.as_deref(), Some("host.example.test."));
    }

    async fn failover_resolver(
        code: ResponseCode,
        parallelism: usize,
        good_response: bool,
    ) -> (DnsResolver, Vec<TestServer>) {
        let mut servers = Vec::new();
        let mut tasks = Vec::new();
        for failing in [true, false] {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let addr = socket.local_addr().unwrap();
            let mut connection = ConnectionConfig::udp();
            connection.port = addr.port();
            servers.push(NameServerConfig::new(addr.ip(), true, vec![connection]));
            tasks.push(TestServer(tokio::spawn(async move {
                let mut buf = [0; 4096];
                loop {
                    let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
                    let query = Message::from_vec(&buf[..n]).unwrap();
                    let mut response = Message::response(query.metadata.id, OpCode::Query);
                    response.add_queries(query.queries.clone());
                    if failing || !good_response {
                        response.metadata.response_code = code;
                    } else {
                        // The refusal must win the race against the valid answer.
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        let query = &query.queries[0];
                        let value = match query.query_type() {
                            RecordType::A => "192.0.2.42",
                            RecordType::AAAA => "2001:db8::42",
                            RecordType::PTR => "host.example.test.",
                            RecordType::MX => "10 mail.example.test.",
                            other => panic!("unexpected query: {other}"),
                        };
                        response.add_answer(Record::from_rdata(
                            query.name().clone(),
                            60,
                            RData::try_from_str(query.query_type(), value).unwrap(),
                        ));
                    }
                    socket
                        .send_to(&response.to_vec().unwrap(), peer)
                        .await
                        .unwrap();
                }
            })));
        }
        let mut options = ResolverOpts::default();
        options.num_concurrent_reqs = parallelism;
        options.timeout = Duration::from_secs(1);
        options.attempts = 1;
        (
            DnsResolver::from_resolver(
                ResolverPool::from_config(ResolverConfig::from_name_servers(servers), options)
                    .unwrap(),
            ),
            tasks,
        )
    }

    #[tokio::test]
    async fn server_refusal_does_not_cancel_slower_valid_answers() {
        for code in [ResponseCode::Refused, ResponseCode::ServFail] {
            for parallelism in [1, 2] {
                let (resolver, _servers) = failover_resolver(code, parallelism, true).await;
                tokio::time::timeout(Duration::from_secs(3), async {
                    let ips = resolver.inner.lookup_ip("public.example.").await.unwrap();
                    assert!(ips
                        .iter()
                        .any(|ip| ip == "192.0.2.42".parse::<std::net::IpAddr>().unwrap()));
                    assert!(ips
                        .iter()
                        .any(|ip| ip == "2001:db8::42".parse::<std::net::IpAddr>().unwrap()));
                    let name = crate::net::dns::reverse_lookup_with_resolver(
                        &resolver.inner,
                        "192.0.2.42".parse().unwrap(),
                        Duration::from_secs(1),
                    )
                    .await;
                    assert_eq!(name.as_deref(), Some("host.example.test."));
                    assert_eq!(
                        resolver.mx("public.example.").await[0].exchange,
                        "mail.example.test."
                    );
                })
                .await
                .unwrap();
            }
        }
    }

    #[tokio::test]
    async fn all_servers_refusing_still_returns_an_error() {
        let (resolver, _servers) = failover_resolver(ResponseCode::Refused, 2, false).await;
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            resolver.inner.lookup_ip("public.example."),
        )
        .await
        .unwrap();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn trusted_nxdomain_remains_terminal() {
        let (resolver, _servers) = failover_resolver(ResponseCode::NXDomain, 2, true).await;
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            resolver.inner.lookup_ip("missing.example."),
        )
        .await
        .unwrap();
        assert!(result.unwrap_err().is_nx_domain());
    }
}
