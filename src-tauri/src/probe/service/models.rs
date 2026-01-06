use serde::{Deserialize, Serialize};

/// Service information detected on a port
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub quic_version: Option<String>,
    pub banner: Option<String>,
    pub raw: Option<String>,
    pub cpes: Vec<String>,
    pub tls_info: Option<TlsInfo>,
}

/// TLS information extracted from a TLS handshake
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlsInfo {
    pub version: Option<String>,
    pub cipher_suite: Option<String>,
    pub alpn: Option<String>,
    pub sni: Option<String>,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    /// Not before date in RFC2822 format
    pub not_before: Option<String>,
    /// Not after date in RFC2822 format
    pub not_after: Option<String>,
    pub san_list: Vec<String>,
    pub serial_hex: Option<String>,
    /// Signature algorithm name
    pub sig_algorithm: Option<String>,
    /// Public key algorithm name
    pub pubkey_algorithm: Option<String>,
}
