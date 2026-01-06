use anyhow::Result;
use hickory_resolver::{
    proto::rr::rdata::{CERT, MX, NS, SOA, SRV, TLSA, TXT},
    TokioResolver,
};
use std::{
    net::{Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use crate::model::dns::{
    CertRecord, DomainLookupInfo, MxRecord, SoaRecord, SrvRecord, TlsaRecord, TxtRecord,
};

pub fn get_resolver() -> Result<TokioResolver> {
    // Use system DNS configuration
    match TokioResolver::builder_tokio() {
        Ok(resolver) => Ok(resolver.build()),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create TokioAsyncResolver: {}",
            e
        )),
    }
}

#[derive(Clone)]
pub struct DnsResolver {
    inner: Arc<TokioResolver>,
}

impl DnsResolver {
    pub fn new() -> Result<Self> {
        let r = get_resolver()?;
        Ok(Self { inner: Arc::new(r) })
    }

    #[allow(unused)]
    pub fn from_resolver(inner: TokioResolver) -> Self {
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
            .ipv4_lookup(&q)
            .await
            .map(|l| l.iter().map(|a| a.0).collect::<Vec<Ipv4Addr>>())
            .unwrap_or_default()
    }

    pub async fn aaaa(&self, name: &str) -> Vec<Ipv6Addr> {
        let q = Self::fqdn(name);
        self.inner
            .ipv6_lookup(&q)
            .await
            .map(|l| l.iter().map(|aaaa| aaaa.0).collect::<Vec<Ipv6Addr>>())
            .unwrap_or_default()
    }

    pub async fn mx(&self, name: &str) -> Vec<MxRecord> {
        let q = Self::fqdn(name);
        self.inner
            .mx_lookup(&q)
            .await
            .map(|l| {
                l.iter()
                    .map(|mx: &MX| MxRecord {
                        preference: mx.preference(),
                        exchange: mx.exchange().to_utf8(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn ns(&self, name: &str) -> Vec<String> {
        let q = Self::fqdn(name);
        self.inner
            .ns_lookup(&q)
            .await
            .map(|l| l.iter().map(|ns: &NS| ns.to_utf8()).collect())
            .unwrap_or_default()
    }

    pub async fn soa(&self, name: &str) -> Vec<SoaRecord> {
        let q = Self::fqdn(name);
        self.inner
            .soa_lookup(&q)
            .await
            .map(|l| {
                l.iter()
                    .map(|soa: &SOA| SoaRecord {
                        mname: soa.mname().to_utf8(),
                        rname: soa.rname().to_utf8(),
                        serial: soa.serial(),
                        refresh: soa.refresh(),
                        retry: soa.retry(),
                        expire: soa.expire(),
                        minimum: soa.minimum(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        let q = Self::fqdn(name);
        self.inner
            .srv_lookup(&q)
            .await
            .map(|l| {
                l.iter()
                    .map(|srv: &SRV| SrvRecord {
                        priority: srv.priority(),
                        weight: srv.weight(),
                        port: srv.port(),
                        target: srv.target().to_utf8(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn tlsa(&self, name: &str) -> Vec<TlsaRecord> {
        let q = Self::fqdn(name);
        self.inner
            .tlsa_lookup(&q)
            .await
            .map(|l| {
                l.iter()
                    .map(|t: &TLSA| TlsaRecord {
                        cert_usage: u8::from(t.cert_usage()),
                        selector: u8::from(t.selector()),
                        matching: u8::from(t.matching()),
                        cert_data_base64: data_encoding::BASE64.encode(t.cert_data()),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn txt(&self, name: &str) -> Vec<TxtRecord> {
        let q = Self::fqdn(name);
        self.inner
            .txt_lookup(&q)
            .await
            .map(|l| {
                l.iter()
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
            .cert_lookup(&q)
            .await
            .map(|l| {
                l.iter()
                    .map(|c: &CERT| CertRecord {
                        cert_type: u16::from(c.cert_type()),
                        key_tag: c.key_tag(),
                        algorithm: u8::from(c.algorithm()),
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
