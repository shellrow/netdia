use super::tls::SkipServerVerification;
use anyhow::Result;
use quinn::{ClientConfig, Endpoint as QuinnEndpoint};
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore};
use std::{net::SocketAddr, sync::Arc, time::Duration};

/// Create a QUIC client configuration with optional certificate verification skipping and ALPN protocols.
pub fn quic_client_config(skip_verify: bool, alpn: &Vec<Vec<u8>>) -> Result<ClientConfig> {
    let mut roots = RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs()? {
        let _ = roots.add(cert);
    }
    let mut tls = RustlsClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    if skip_verify {
        tls.dangerous()
            .set_certificate_verifier(SkipServerVerification::new());
    }
    tls.enable_early_data = true;
    tls.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
    let client_conf = quinn::crypto::rustls::QuicClientConfig::try_from(tls)?;
    Ok(ClientConfig::new(Arc::new(client_conf)))
}

/// Configuration options for a QUIC socket.
#[derive(Debug, Clone)]
pub struct QuicConfig {
    pub skip_verify: bool,
    pub alpn: Vec<Vec<u8>>,
    pub family: super::SocketFamily,
}

/// Asynchronous QUIC socket built on quinn and tokio.
#[derive(Debug)]
pub struct AsyncQuicSocket {
    inner: QuinnEndpoint,
}

impl AsyncQuicSocket {
    /// Create an asynchronous QUIC socket from the given configuration.
    pub fn from_config(config: &QuicConfig) -> Result<Self> {
        let client_cfg = quic_client_config(config.skip_verify, &config.alpn)?;
        let mut endpoint = QuinnEndpoint::client(
            (if config.family.is_v6() {
                "[::]:0"
            } else {
                "0.0.0.0:0"
            })
            .parse()
            .unwrap(),
        )?;
        endpoint.set_default_client_config(client_cfg);
        Ok(Self { inner: endpoint })
    }

    /// Connect to the specified remote address using QUIC.
    pub async fn connect(
        &self,
        remote_addr: &SocketAddr,
        server_name: &str,
    ) -> Result<quinn::Connection> {
        let conn = self.inner.connect(*remote_addr, server_name)?.await?;
        Ok(conn)
    }

    pub async fn connect_timeout(
        &self,
        remote_addr: &SocketAddr,
        server_name: &str,
        timeout: Duration,
    ) -> Result<quinn::Connection> {
        let conn_fut = self.inner.connect(*remote_addr, server_name)?;
        let conn = tokio::time::timeout(timeout, conn_fut).await??;
        Ok(conn)
    }
}
