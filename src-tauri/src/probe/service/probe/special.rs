use crate::probe::service::models::ServiceInfo;
use crate::probe::service::payload::{PayloadBuilder, PayloadContext};
use crate::probe::service::probe::{PortProbeResult, ProbeContext, ServiceProbe};
use crate::probe::service::read_timeout;
use anyhow::{bail, Result};
use std::net::SocketAddr;
use tokio::{io::AsyncWriteExt, net::TcpStream, time::timeout};

pub struct SpecialTcpProbe;

impl SpecialTcpProbe {
    pub async fn run(ctx: ProbeContext) -> Result<PortProbeResult> {
        let addr: SocketAddr = SocketAddr::new(ctx.ip, ctx.probe.port);
        let mut stream = timeout(ctx.timeout, TcpStream::connect(addr)).await??;

        let payload = PayloadBuilder::new(ctx.probe.clone())
            .payload(PayloadContext::default())
            .unwrap_or_default();
        if !payload.is_empty() {
            timeout(ctx.timeout, stream.write_all(&payload)).await??;
            stream.flush().await?;
        }

        let bytes = read_timeout(&mut stream, ctx.timeout, ctx.timeout, ctx.max_read_size).await?;
        let service_info = match ctx.probe.probe_id {
            ServiceProbe::TcpImapCapability => text_service_info("imap", &bytes),
            ServiceProbe::TcpMssqlPrelogin => mssql_service_info(&bytes),
            ServiceProbe::TcpMemcachedVersion => text_service_info("memcached", &bytes),
            ServiceProbe::TcpMqttConnect => mqtt_service_info(&bytes),
            ServiceProbe::TcpMySqlHandshake => mysql_service_info(&bytes),
            ServiceProbe::TcpOracleTns => oracle_service_info(&bytes),
            ServiceProbe::TcpPop3Capa => text_service_info("pop3", &bytes),
            ServiceProbe::TcpPostgresSslReq => postgres_service_info(&bytes),
            ServiceProbe::TcpRedisPing => text_service_info("redis", &bytes),
            ServiceProbe::TcpRdpNegotiation => rdp_service_info(&bytes),
            ServiceProbe::TcpSmbNegotiation => smb_service_info(&bytes),
            ServiceProbe::TcpSmtpEhlo => text_service_info("smtp", &bytes),
            _ => bail!("unsupported special probe: {:?}", ctx.probe.probe_id),
        };

        Ok(PortProbeResult {
            ip: ctx.ip,
            hostname: ctx.hostname,
            port: ctx.probe.port,
            transport: ctx.probe.transport,
            probe_id: ctx.probe.probe_id,
            service_info,
        })
    }
}

fn text_service_info(service: &str, raw: &[u8]) -> ServiceInfo {
    let text = String::from_utf8_lossy(raw).to_string();
    let banner = first_visible_line(&text).unwrap_or_else(|| text.trim().to_string());

    ServiceInfo {
        name: Some(service.to_string()),
        banner: (!banner.is_empty()).then_some(banner),
        raw: Some(text),
        ..Default::default()
    }
}

fn postgres_service_info(raw: &[u8]) -> ServiceInfo {
    let banner = match raw.first().copied() {
        Some(b'S') => "PostgreSQL SSLRequest accepted".to_string(),
        Some(b'N') => "PostgreSQL SSLRequest rejected".to_string(),
        Some(byte) => format!("PostgreSQL SSLRequest response=0x{byte:02x}"),
        None => "PostgreSQL SSLRequest empty response".to_string(),
    };

    ServiceInfo {
        name: Some("postgresql".to_string()),
        banner: Some(banner.clone()),
        raw: Some(format!("{raw:?}")),
        ..Default::default()
    }
}

fn mysql_service_info(raw: &[u8]) -> ServiceInfo {
    let (version, banner) = parse_mysql_banner(raw)
        .map(|version| {
            let banner = format!("MySQL handshake version={version}");
            (Some(version), banner)
        })
        .unwrap_or_else(|| (None, format!("MySQL handshake {} bytes", raw.len())));

    ServiceInfo {
        name: Some("mysql".to_string()),
        version,
        banner: Some(banner.clone()),
        raw: Some(String::from_utf8_lossy(raw).to_string()),
        ..Default::default()
    }
}

fn mqtt_service_info(raw: &[u8]) -> ServiceInfo {
    let banner =
        parse_mqtt_banner(raw).unwrap_or_else(|| format!("MQTT response {} bytes", raw.len()));

    ServiceInfo {
        name: Some("mqtt".to_string()),
        banner: Some(banner.clone()),
        raw: Some(format!("{raw:?}")),
        ..Default::default()
    }
}

fn smb_service_info(raw: &[u8]) -> ServiceInfo {
    let banner =
        parse_smb_banner(raw).unwrap_or_else(|| format!("SMB response {} bytes", raw.len()));

    ServiceInfo {
        name: Some("smb".to_string()),
        banner: Some(banner.clone()),
        raw: Some(format!("{raw:?}")),
        ..Default::default()
    }
}

fn oracle_service_info(raw: &[u8]) -> ServiceInfo {
    let banner = parse_oracle_banner(raw)
        .unwrap_or_else(|| format!("Oracle TNS response {} bytes", raw.len()));

    ServiceInfo {
        name: Some("oracle".to_string()),
        banner: Some(banner.clone()),
        raw: Some(format!("{raw:?}")),
        ..Default::default()
    }
}

fn rdp_service_info(raw: &[u8]) -> ServiceInfo {
    let banner =
        parse_rdp_banner(raw).unwrap_or_else(|| format!("RDP response {} bytes", raw.len()));

    ServiceInfo {
        name: Some("rdp".to_string()),
        banner: Some(banner.clone()),
        raw: Some(banner),
        ..Default::default()
    }
}

fn mssql_service_info(raw: &[u8]) -> ServiceInfo {
    let parsed = parse_tds_prelogin_response(raw);
    let banner = parsed
        .as_ref()
        .map(TdsPreloginResponse::banner)
        .unwrap_or_else(|| format!("TDS prelogin response {} bytes", raw.len()));

    ServiceInfo {
        name: Some("mssql".to_string()),
        version: parsed.as_ref().and_then(|value| value.version.clone()),
        banner: Some(banner.clone()),
        raw: Some(banner),
        ..Default::default()
    }
}

fn first_visible_line(text: &str) -> Option<String> {
    text.lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn parse_mysql_banner(raw: &[u8]) -> Option<String> {
    if raw.len() < 6 {
        return None;
    }

    let payload = if raw.len() > 4 { &raw[4..] } else { raw };
    let protocol = *payload.first()?;
    if protocol == 0x00 {
        return None;
    }

    let version_end = payload.iter().skip(1).position(|byte| *byte == 0x00)? + 1;
    let version = String::from_utf8_lossy(payload.get(1..version_end)?).to_string();
    Some(format!("protocol={protocol} server={version}"))
}

fn parse_mqtt_banner(raw: &[u8]) -> Option<String> {
    if raw.len() < 4 || raw[0] != 0x20 {
        return None;
    }

    let return_code = match raw[3] {
        0x00 => "accepted",
        0x01 => "refused-unacceptable-protocol",
        0x02 => "refused-identifier-rejected",
        0x03 => "refused-server-unavailable",
        0x04 => "refused-bad-credentials",
        0x05 => "refused-not-authorized",
        other => return Some(format!("MQTT CONNACK return_code=0x{other:02x}")),
    };

    Some(format!("MQTT CONNACK return_code={return_code}"))
}

#[derive(Debug)]
struct TdsPreloginResponse {
    version: Option<String>,
    encryption: Option<String>,
    instance: Option<String>,
    mars: Option<String>,
}

impl TdsPreloginResponse {
    fn banner(&self) -> String {
        let mut parts = vec!["TDS prelogin".to_string()];
        if let Some(version) = &self.version {
            parts.push(format!("version={version}"));
        }
        if let Some(encryption) = &self.encryption {
            parts.push(format!("encryption={encryption}"));
        }
        if let Some(instance) = &self.instance {
            parts.push(format!("instance={instance}"));
        }
        if let Some(mars) = &self.mars {
            parts.push(format!("mars={mars}"));
        }
        parts.join(" ")
    }
}

fn parse_tds_prelogin_response(raw: &[u8]) -> Option<TdsPreloginResponse> {
    if raw.len() < 8 {
        return None;
    }

    let payload = &raw[8..];
    let mut index = 0usize;
    let mut version = None;
    let mut encryption = None;
    let mut instance = None;
    let mut mars = None;

    while index < payload.len() {
        let token = *payload.get(index)?;
        if token == 0xff {
            break;
        }

        let offset =
            u16::from_be_bytes([*payload.get(index + 1)?, *payload.get(index + 2)?]) as usize;
        let length =
            u16::from_be_bytes([*payload.get(index + 3)?, *payload.get(index + 4)?]) as usize;
        let start = offset;
        let end = start.checked_add(length)?;
        let value = payload.get(start..end)?;

        match token {
            0x00 if value.len() >= 6 => {
                let major = value[0];
                let minor = value[1];
                let build = u16::from_be_bytes([value[2], value[3]]);
                let subbuild = u16::from_be_bytes([value[4], value[5]]);
                if subbuild == 0 {
                    version = Some(format!("{major}.{minor}.{build}"));
                } else {
                    version = Some(format!("{major}.{minor}.{build}.{subbuild}"));
                }
            }
            0x01 if !value.is_empty() => {
                encryption = Some(match value[0] {
                    0x00 => "encrypt-off".to_string(),
                    0x01 => "encrypt-on".to_string(),
                    0x02 => "encrypt-not-supported".to_string(),
                    0x03 => "encrypt-required".to_string(),
                    other => format!("unknown(0x{other:02x})"),
                });
            }
            0x02 => {
                let trimmed = value.split(|byte| *byte == 0x00).next().unwrap_or_default();
                if !trimmed.is_empty() {
                    instance = Some(String::from_utf8_lossy(trimmed).to_string());
                }
            }
            0x04 if !value.is_empty() => {
                mars = Some(if value[0] == 0x00 {
                    "off".to_string()
                } else {
                    "on".to_string()
                });
            }
            _ => {}
        }

        index += 5;
    }

    Some(TdsPreloginResponse {
        version,
        encryption,
        instance,
        mars,
    })
}

fn parse_smb_banner(raw: &[u8]) -> Option<String> {
    if raw.len() < 74 {
        return None;
    }
    let offset = if raw.starts_with(&[0x00, 0x00]) || raw[0] == 0x00 {
        4
    } else {
        0
    };
    if raw.get(offset..offset + 4)? != b"\xfeSMB" {
        return None;
    }
    let security_mode = u16::from_le_bytes([*raw.get(offset + 70)?, *raw.get(offset + 71)?]);
    let dialect = u16::from_le_bytes([*raw.get(offset + 72)?, *raw.get(offset + 73)?]);
    Some(format!(
        "SMB2 negotiate dialect={} signing={}",
        smb_dialect_name(dialect),
        smb_security_mode_label(security_mode)
    ))
}

fn smb_dialect_name(dialect: u16) -> &'static str {
    match dialect {
        0x0202 => "2.0.2",
        0x0210 => "2.1",
        0x0300 => "3.0",
        0x0302 => "3.0.2",
        0x0311 => "3.1.1",
        _ => "unknown",
    }
}

fn smb_security_mode_label(security_mode: u16) -> &'static str {
    match security_mode & 0x0003 {
        0x0000 => "disabled",
        0x0001 => "enabled",
        0x0002 => "required",
        0x0003 => "enabled+required",
        _ => "unknown",
    }
}

fn parse_oracle_banner(raw: &[u8]) -> Option<String> {
    if raw.len() < 8 {
        return None;
    }
    let packet_length = u16::from_be_bytes([raw[0], raw[1]]);
    let packet_type = raw[4];
    let packet_type_label = match packet_type {
        2 => "accept",
        4 => "refuse",
        5 => "redirect",
        6 => "data",
        11 => "resend",
        _ => "unknown",
    };

    Some(format!(
        "Oracle TNS packet_type={packet_type_label} packet_length={packet_length}"
    ))
}

fn parse_rdp_banner(raw: &[u8]) -> Option<String> {
    if raw.len() < 11 || raw[0] != 0x03 || raw[1] != 0x00 {
        return None;
    }

    let tpkt_len = u16::from_be_bytes([raw[2], raw[3]]);
    let x224_type = raw.get(5).copied()?;
    let x224_label = match x224_type {
        0xd0 => "confirm",
        0xe0 => "request",
        _ => "response",
    };

    if raw.len() >= 19 && raw[11] == 0x02 {
        let selected_protocol = u32::from_be_bytes([raw[15], raw[16], raw[17], raw[18]]);
        return Some(format!(
            "RDP X.224 {x224_label} tpkt_len={tpkt_len} selected_protocol={}",
            rdp_protocol_name(selected_protocol)
        ));
    }

    if raw.len() >= 19 && raw[11] == 0x03 {
        let failure_code = u32::from_be_bytes([raw[15], raw[16], raw[17], raw[18]]);
        return Some(format!(
            "RDP X.224 {x224_label} tpkt_len={tpkt_len} negotiation_failure={failure_code}"
        ));
    }

    Some(format!("RDP X.224 {x224_label} tpkt_len={tpkt_len}"))
}

fn rdp_protocol_name(protocol: u32) -> &'static str {
    match protocol {
        0x0000_0000 => "rdp",
        0x0000_0001 => "ssl",
        0x0000_0002 => "hybrid",
        0x0000_0004 => "rdstls",
        0x0000_0008 => "hybrid-ex",
        _ => "unknown",
    }
}
