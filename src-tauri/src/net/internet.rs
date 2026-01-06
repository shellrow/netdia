use std::time::Duration;

use crate::model::{IpInfo, IpInfoDual};
use anyhow::{Context, Result};
use reqwest::Client;

const IPSTRUCT_URL: &str = "https://api.ipstruct.com/ip";
const IPSTRUCT_V4_URL: &str = "https://ipv4.ipstruct.com/ip";
//const IP_VERSION_4: &str = "v4";
const IP_VERSION_6: &str = "v6";

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
    let info: IpInfo = resp.json().await.context("parse json IpInfo")?;
    Ok(Some(info))
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
