//! DNS failover across the servers discovered in the system configuration.

use futures::{future::BoxFuture, stream, FutureExt, StreamExt};
use hickory_resolver::{
    config::{ResolverConfig, ResolverOpts},
    lookup::Lookup,
    lookup_ip::LookupIp,
    net::{runtime::TokioRuntimeProvider, DnsError, NetError},
    proto::{
        op::{Query, ResponseCode},
        rr::{Name, RData, RecordType},
    },
    TokioResolver,
};
use std::{net::IpAddr, time::Duration};

struct Server {
    resolver: TokioResolver,
    address: IpAddr,
    trust_negative: bool,
}

pub struct ResolverPool {
    servers: Vec<Server>,
    config: ResolverConfig,
    options: ResolverOpts,
}

impl ResolverPool {
    pub fn from_system() -> Result<Self, NetError> {
        let (config, options) = hickory_resolver::system_conf::read_system_conf()?;
        Self::from_config(config, options)
    }

    pub(super) fn from_config(
        config: ResolverConfig,
        options: ResolverOpts,
    ) -> Result<Self, NetError> {
        let mut servers = Vec::new();
        for server in config.name_servers() {
            // Search expansion belongs to the pool. Each server receives the same absolute
            // candidate, so a search-suffix NXDOMAIN cannot conceal an earlier REFUSED.
            let single = ResolverConfig::from_name_servers(vec![server.clone()]);
            let resolver =
                TokioResolver::builder_with_config(single, TokioRuntimeProvider::default())
                    .with_options(options.clone())
                    .build()?;
            servers.push(Server {
                resolver,
                address: server.ip,
                trust_negative: server.trust_negative_responses,
            });
        }
        if servers.is_empty() {
            return Err(NetError::NoConnections);
        }
        Ok(Self {
            servers,
            config,
            options,
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.options.timeout = timeout;
        self
    }

    fn candidates(&self, host: &str) -> Result<Vec<Name>, NetError> {
        let name = Name::from_utf8(host)?;
        let mut absolute = name.clone();
        absolute.set_fqdn(true);
        if name.is_fqdn() || absolute.to_ascii().ends_with(".onion.") {
            return Ok(vec![absolute]);
        }
        let raw_first = name.num_labels() as usize > self.options.ndots || name.is_localhost();
        let mut names = Vec::new();
        if raw_first {
            names.push(absolute.clone());
        }
        // Hickory gives the local domain precedence over the search list.
        for suffix in self.config.domain().into_iter().chain(self.config.search()) {
            if let Ok(mut candidate) = name.clone().append_domain(suffix) {
                candidate.set_fqdn(true);
                if !names.contains(&candidate) {
                    names.push(candidate);
                }
            }
        }
        if !names.contains(&absolute) {
            names.push(absolute);
        }
        Ok(names)
    }

    async fn run<T: Send, F>(&self, host: &str, query: F) -> Result<T, NetError>
    where
        F: Sync + for<'a> Fn(&'a TokioResolver, Name) -> BoxFuture<'a, Result<T, NetError>>,
    {
        // One budget covers all servers and search candidates. Dropping this future also
        // drops the outstanding lookups; no application retry task survives cancellation.
        tokio::time::timeout(self.options.timeout, async {
            let mut last = NetError::NoConnections;
            for name in self.candidates(host)? {
                match self.query_candidate(name, &query).await {
                    Ok(value) => return Ok(value),
                    Err(error) if negative_answer(&error) => last = error,
                    Err(error) => return Err(error),
                }
            }
            Err(last)
        })
        .await
        .map_err(|_| NetError::Timeout)?
    }

    async fn query_candidate<T: Send, F>(&self, name: Name, query: &F) -> Result<T, NetError>
    where
        F: Sync + for<'a> Fn(&'a TokioResolver, Name) -> BoxFuture<'a, Result<T, NetError>>,
    {
        let concurrency = self
            .options
            .num_concurrent_reqs
            .max(1)
            .min(self.servers.len());
        let batches = self.servers.len().div_ceil(concurrency);
        // Reserve time for later servers when an entire earlier batch is unresponsive.
        let attempt_budget = self.options.timeout / u32::try_from(batches).unwrap_or(u32::MAX);
        let mut pending = Vec::new();
        for server in &self.servers {
            let name = name.clone();
            pending.push(
                async move {
                    let started = tokio::time::Instant::now();
                    let result =
                        tokio::time::timeout(attempt_budget, query(&server.resolver, name.clone()))
                            .await
                            .unwrap_or(Err(NetError::Timeout));
                    tracing::debug!(server = %server.address, query = %name,
                    elapsed_ms = started.elapsed().as_millis(), error = ?result.as_ref().err(),
                    "DNS server lookup completed");
                    (server.trust_negative, result)
                }
                .boxed(),
            );
        }
        let mut requests = stream::iter(pending).buffer_unordered(concurrency);
        let mut last = NetError::NoConnections;
        while let Some((trust_negative, result)) = requests.next().await {
            match result {
                Ok(value) => return Ok(value),
                Err(error) if negative_answer(&error) && !trust_negative => {
                    // An untrusted negative must not overwrite a server failure and
                    // accidentally permit search expansion after an inconclusive round.
                    if matches!(last, NetError::NoConnections) || negative_answer(&last) {
                        last = error;
                    }
                }
                Err(error) if retryable(&error) => last = error,
                Err(error) => return Err(error),
            }
        }
        Err(last)
    }

    pub async fn lookup_ip(&self, host: &str) -> Result<LookupIp, NetError> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            let data = RData::from(ip);
            return Ok(
                Lookup::from_rdata(Query::query(Name::root(), data.record_type()), data).into(),
            );
        }
        self.run(host, |resolver, name| {
            async move { resolver.lookup_ip(name).await }.boxed()
        })
        .await
    }

    pub async fn reverse_lookup(&self, ip: IpAddr) -> Result<Lookup, NetError> {
        self.lookup(&Name::from(ip).to_ascii(), RecordType::PTR)
            .await
    }

    pub async fn lookup(&self, host: &str, record_type: RecordType) -> Result<Lookup, NetError> {
        self.run(host, move |resolver, name| {
            async move { resolver.lookup(name, record_type).await }.boxed()
        })
        .await
    }
}

fn negative_answer(error: &NetError) -> bool {
    matches!(error, NetError::Dns(DnsError::NoRecordsFound(records))
        if matches!(records.response_code, ResponseCode::NXDomain | ResponseCode::NoError))
}

fn retryable(error: &NetError) -> bool {
    matches!(
        error,
        NetError::Dns(DnsError::ResponseCode(
            ResponseCode::Refused | ResponseCode::ServFail
        )) | NetError::Io(_)
            | NetError::Timeout
            | NetError::NoConnections
            | NetError::Busy
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_resolver::{
        config::{ConnectionConfig, LookupIpStrategy, NameServerConfig},
        proto::{
            op::{Message, OpCode, Query},
            rr::{RData, Record},
        },
    };
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tokio::net::UdpSocket;

    struct TestServer {
        config: NameServerConfig,
        queries: Arc<Mutex<Vec<String>>>,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn server(
        reply: impl Fn(&Query) -> Option<(ResponseCode, Option<&'static str>)> + Send + 'static,
    ) -> TestServer {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut connection = ConnectionConfig::udp();
        connection.port = address.port();
        let queries = Arc::new(Mutex::new(Vec::new()));
        let captured = queries.clone();
        let task = tokio::spawn(async move {
            let mut buf = [0; 2048];
            loop {
                let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
                let request = Message::from_vec(&buf[..n]).unwrap();
                let query = &request.queries[0];
                captured.lock().unwrap().push(query.name().to_ascii());
                let Some((code, value)) = reply(query) else {
                    continue;
                };
                let mut response = Message::response(request.metadata.id, OpCode::Query);
                response.add_query(query.clone());
                response.metadata.response_code = code;
                if let Some(value) = value {
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
        });
        TestServer {
            config: NameServerConfig::new(address.ip(), true, vec![connection]),
            queries,
            task,
        }
    }

    fn options(timeout: Duration) -> ResolverOpts {
        let mut options = ResolverOpts::default();
        options.timeout = timeout;
        options.attempts = 1;
        options.ip_strategy = LookupIpStrategy::Ipv4Only;
        options
    }

    #[tokio::test]
    async fn refusal_cannot_be_hidden_by_search_suffix_nxdomain() {
        let restricted = server(|query| {
            Some((
                if query.name().to_ascii() == "public.example." {
                    ResponseCode::Refused
                } else {
                    ResponseCode::NXDomain
                },
                None,
            ))
        })
        .await;
        let public = server(|query| {
            Some(if query.name().to_ascii() == "public.example." {
                (ResponseCode::NoError, Some("192.0.2.42"))
            } else {
                (ResponseCode::NXDomain, None)
            })
        })
        .await;
        let mut config = ResolverConfig::from_name_servers(vec![
            restricted.config.clone(),
            public.config.clone(),
        ]);
        config.add_search(Name::from_ascii("corp.test.").unwrap());
        let mut opts = options(Duration::from_secs(1));
        opts.num_concurrent_reqs = 1;
        let pool = ResolverPool::from_config(config, opts).unwrap();
        let ips = pool.lookup_ip("public.example").await.unwrap();
        assert_eq!(
            ips.iter().collect::<Vec<_>>(),
            vec!["192.0.2.42".parse::<IpAddr>().unwrap()]
        );
        assert!(restricted
            .queries
            .lock()
            .unwrap()
            .iter()
            .all(|q| q == "public.example."));
        assert!(public
            .queries
            .lock()
            .unwrap()
            .iter()
            .all(|q| q == "public.example."));
    }

    #[tokio::test]
    async fn search_order_ndots_and_absolute_names_are_preserved() {
        let dns = server(|query| {
            Some(if query.name().to_ascii() == "host.second.test." {
                (ResponseCode::NoError, Some("192.0.2.42"))
            } else {
                (ResponseCode::NXDomain, None)
            })
        })
        .await;
        let mut config = ResolverConfig::from_name_servers(vec![dns.config.clone()]);
        config.add_search(Name::from_ascii("first.test.").unwrap());
        config.add_search(Name::from_ascii("second.test.").unwrap());
        let pool = ResolverPool::from_config(config, options(Duration::from_secs(1))).unwrap();
        pool.lookup_ip("host").await.unwrap();
        assert_eq!(
            *dns.queries.lock().unwrap(),
            vec!["host.first.test.", "host.second.test."]
        );
        assert_eq!(
            pool.candidates("host.")
                .unwrap()
                .iter()
                .map(Name::to_ascii)
                .collect::<Vec<_>>(),
            vec!["host."]
        );
        assert_eq!(
            pool.candidates("public.example").unwrap()[0].to_ascii(),
            "public.example."
        );
        assert_eq!(
            pool.candidates("localhost").unwrap()[0].to_ascii(),
            "localhost."
        );
    }

    #[tokio::test]
    async fn nodata_is_not_treated_as_a_server_failure() {
        let negative = server(|_| Some((ResponseCode::NoError, None))).await;
        let positive = server(|_| Some((ResponseCode::NoError, Some("192.0.2.42")))).await;
        let mut opts = options(Duration::from_secs(1));
        opts.num_concurrent_reqs = 1;
        let pool = ResolverPool::from_config(
            ResolverConfig::from_name_servers(vec![
                negative.config.clone(),
                positive.config.clone(),
            ]),
            opts,
        )
        .unwrap();
        assert!(negative_answer(
            &pool.lookup("host.", RecordType::A).await.unwrap_err()
        ));
        assert!(positive.queries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn silent_servers_leave_time_for_the_remaining_server() {
        let silent1 = server(|_| None).await;
        let silent2 = server(|_| None).await;
        let good = server(|_| Some((ResponseCode::NoError, Some("192.0.2.42")))).await;
        let pool = ResolverPool::from_config(
            ResolverConfig::from_name_servers(vec![
                silent1.config.clone(),
                silent2.config.clone(),
                good.config.clone(),
            ]),
            options(Duration::from_millis(400)),
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), pool.lookup_ip("host."))
            .await
            .unwrap()
            .unwrap();
        assert!(!good.queries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn overall_timeout_does_not_grow_with_server_or_search_count() {
        let silent = server(|_| None).await;
        let mut config = ResolverConfig::from_name_servers(vec![silent.config.clone(); 4]);
        config.add_search(Name::from_ascii("first.test.").unwrap());
        config.add_search(Name::from_ascii("second.test.").unwrap());
        let pool = ResolverPool::from_config(config, options(Duration::from_millis(80))).unwrap();
        let result = tokio::time::timeout(Duration::from_millis(300), pool.lookup_ip("host"))
            .await
            .unwrap();
        assert!(matches!(result, Err(NetError::Timeout)));
    }

    #[tokio::test]
    async fn literals_and_hosts_file_do_not_require_a_dns_response() {
        let silent = server(|_| None).await;
        let mut opts = options(Duration::from_secs(1));
        opts.ndots = 6;
        let pool = ResolverPool::from_config(
            ResolverConfig::from_name_servers(vec![silent.config.clone()]),
            opts,
        )
        .unwrap();
        for ip in ["192.0.2.42", "2001:db8::42"] {
            assert_eq!(
                pool.lookup_ip(ip).await.unwrap().iter().next(),
                Some(ip.parse::<IpAddr>().unwrap())
            );
        }
        assert!(pool
            .lookup_ip("localhost")
            .await
            .unwrap()
            .iter()
            .any(|ip| ip.is_loopback()));
        assert!(silent.queries.lock().unwrap().is_empty());
    }
}
