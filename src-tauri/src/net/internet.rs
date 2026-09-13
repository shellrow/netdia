use std::{net::IpAddr, time::Duration};

use crate::model::{IpInfo, IpInfoDual};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

const IPSTRUCT_URL: &str = "https://api.ipstruct.com/v2/ip";
const IPSTRUCT_V4_URL: &str = "https://ipv4.ipstruct.com/v2/ip";
const IP_VERSION_6: &str = "v6";

/// The v2 fields used by netdia; additional IP intelligence fields are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IpStructV2Info {
    ip: IpAddr,
    version: u8,
    network: Option<String>,
    asn: Option<u32>,
    as_name: Option<String>,
    country: Option<IpStructCountry>,
}

#[derive(Debug, Deserialize)]
struct IpStructCountry {
    code: String,
    name: Option<String>,
}

impl TryFrom<IpStructV2Info> for IpInfo {
    type Error = anyhow::Error;

    fn try_from(info: IpStructV2Info) -> Result<Self> {
        let (version, decimal) = match info.ip {
            IpAddr::V4(ip) => (4, u32::from(ip).to_string()),
            IpAddr::V6(ip) => (6, u128::from(ip).to_string()),
        };
        anyhow::ensure!(
            info.version == version,
            "IP Struct version does not match IP address"
        );
        let (country_code, country_name) = info
            .country
            .map(|country| (country.code, country.name.unwrap_or_default()))
            .unwrap_or_default();

        // Preserve the app's IPC contract while adapting v2's typed, nullable fields.
        Ok(Self {
            ip_version: format!("v{version}"),
            ip_addr_dec: decimal,
            ip_addr: info.ip.to_string(),
            // The non-reverse lookup does not provide a hostname in either API version.
            host_name: String::new(),
            network: info.network.unwrap_or_default(),
            asn: info.asn.map(|asn| asn.to_string()).unwrap_or_default(),
            as_name: info.as_name.unwrap_or_default(),
            country_code,
            country_name,
        })
    }
}

/// Fetch IP information from a given URL
async fn fetch_public_ip(client: &Client, url: &str) -> Result<Option<IpInfo>> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {}", url))?;
    if !resp.status().is_success() {
        anyhow::bail!("{} -> HTTP {}", url, resp.status());
    }
    let info: IpStructV2Info = resp.json().await.context("parse IP Struct v2 response")?;
    Ok(Some(info.try_into()?))
}

fn is_ipv6(info: &IpInfo) -> bool {
    info.ip_version == IP_VERSION_6 || info.ip_addr.contains(':')
}

/// Get public IP information (both IPv4 and IPv6 if available)
pub async fn get_public_ip() -> Result<IpInfoDual> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("build http client")?;

    let v4: Option<IpInfo>;
    let mut v6: Option<IpInfo> = None;

    let (any_res, v4_res) = tokio::join!(
        fetch_public_ip(&client, IPSTRUCT_URL),
        fetch_public_ip(&client, IPSTRUCT_V4_URL),
    );

    let any: Option<IpInfo> = any_res.unwrap_or(None);
    let v4opt: Option<IpInfo> = v4_res.unwrap_or(None);

    match any {
        Some(info) if is_ipv6(&info) => {
            v6 = Some(info);
            v4 = v4opt;
        }
        Some(info) => {
            v4 = Some(info);
        }
        None => {
            v4 = v4opt;
        }
    }
    Ok(IpInfoDual { ipv4: v4, ipv6: v6 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v2_ipv4_preserves_display_fields() {
        let response = json!({
            "ip": "1.1.1.1", "version": 4,
            "network": "1.1.1.0/24", "asn": 13335,
            "asName": "Cloudflare, Inc.", "asDomain": "cloudflare.com",
            "country": { "code": "US", "name": "United States of America" },
            "location": null, "rir": "apnic", "rpki": "valid", "cloud": null,
            "privacy": { "hosting": true, "vpn": false, "tor": false,
                "proxy": false, "relay": false, "anonymous": false }
        });
        let info =
            IpInfo::try_from(serde_json::from_value::<IpStructV2Info>(response).unwrap()).unwrap();
        assert_eq!(info.ip_addr, "1.1.1.1");
        assert_eq!(info.ip_addr_dec, "16843009");
        assert_eq!(info.ip_version, "v4");
        assert!(!is_ipv6(&info));
        assert_eq!(info.network, "1.1.1.0/24");
        assert_eq!(info.asn, "13335");
        assert_eq!(info.as_name, "Cloudflare, Inc.");
        assert_eq!(info.country_code, "US");
        assert_eq!(info.country_name, "United States of America");
        assert!(info.host_name.is_empty());
    }

    #[test]
    fn v2_ipv6_with_unknown_metadata_remains_available() {
        let response = json!({
            "ip": "2001:db8::1", "version": 6,
            "network": null, "asn": null, "asName": null, "country": null
        });
        let info =
            IpInfo::try_from(serde_json::from_value::<IpStructV2Info>(response).unwrap()).unwrap();
        assert_eq!(info.ip_addr, "2001:db8::1");
        assert_eq!(info.ip_addr_dec, "42540766411282592856903984951653826561");
        assert_eq!(info.ip_version, "v6");
        assert!(is_ipv6(&info));
        assert!(info.network.is_empty());
        assert!(info.asn.is_empty());
        assert!(info.as_name.is_empty());
        assert!(info.country_code.is_empty());
        assert!(info.country_name.is_empty());
    }

    #[test]
    fn v2_preserves_zero_asn_and_country_with_unknown_name() {
        let response = json!({
            "ip": "192.0.2.1", "version": 4, "asn": 0,
            "country": { "code": "US", "name": null }
        });
        let info =
            IpInfo::try_from(serde_json::from_value::<IpStructV2Info>(response).unwrap()).unwrap();
        assert_eq!(info.asn, "0");
        assert_eq!(info.country_code, "US");
        assert!(info.country_name.is_empty());
    }

    #[test]
    fn invalid_v2_identity_is_rejected() {
        for response in [
            json!({ "ip": "192.0.2.1", "version": 6 }),
            json!({ "ip": "2001:db8::1", "version": 5 }),
        ] {
            let info = serde_json::from_value::<IpStructV2Info>(response).unwrap();
            assert!(IpInfo::try_from(info).is_err());
        }
        for response in [
            json!({ "ip": "invalid", "version": 4 }),
            json!({ "error": { "code": "rate_limited", "message": "Try later" } }),
            json!({ "ip_addr": "192.0.2.1", "ip_version": "v4" }),
        ] {
            assert!(serde_json::from_value::<IpStructV2Info>(response).is_err());
        }
    }
}
