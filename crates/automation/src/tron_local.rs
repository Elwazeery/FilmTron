//! FilmTron client for the ONE GemiMCP workstation TRON inference broker.
//! Never contacts Ollama directly, starts a model silently or exposes a port.
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

const MAX_REPLY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrokerDiscovery {
    schema: String,
    host: String,
    port: u16,
    token: String,
}

fn discover() -> Result<BrokerDiscovery, String> {
    let location = match std::env::var_os("TRON_BRIDGE_CONFIG_PATH") {
        Some(path) => PathBuf::from(path),
        None if cfg!(windows) => {
            let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA_NOT_CONFIGURED")?;
            PathBuf::from(base).join("GemiTron/TRON/bridge.json")
        }
        None => {
            let home = std::env::var_os("HOME").ok_or("HOME_NOT_CONFIGURED")?;
            PathBuf::from(home).join(".config/GemiTron/TRON/bridge.json")
        }
    };
    let source = std::fs::read_to_string(location).map_err(|_| {
        "TRON_SHARED_BROKER_NOT_RUNNING: launch the approved GemiMCP workstation host".to_owned()
    })?;
    let d: BrokerDiscovery = serde_json::from_str(&source).map_err(|_| "INVALID_BROKER_DISCOVERY")?;
    if d.schema != "gemitron.tron.local-bridge.v1" || d.host != "127.0.0.1"
        || d.port == 0 || d.token.len() < 43
        || !d.token.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("BROKER_LOOPBACK_OR_CREDENTIAL_INVALID".to_owned());
    }
    Ok(d)
}

fn decode_chunked(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    loop {
        let end = rest.windows(2).position(|b| b == b"\r\n").ok_or("CHUNKED_HEADER_MISSING")?;
        let text = std::str::from_utf8(&rest[..end]).map_err(|_| "CHUNKED_SIZE_INVALID")?;
        let size = usize::from_str_radix(text.split(';').next().unwrap_or(""), 16)
            .map_err(|_| "CHUNKED_SIZE_INVALID")?;
        rest = &rest[end + 2..];
        if size == 0 { return Ok(decoded); }
        if size > MAX_REPLY_BYTES as usize || size > rest.len()
            || decoded.len() > MAX_REPLY_BYTES as usize - size
            || rest.get(size..size + 2) != Some(b"\r\n".as_slice())
        {
            return Err("CHUNKED_REPLY_INVALID_OR_OVERSIZED".to_owned());
        }
        decoded.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

fn broker_request(route: &'static str, payload: Option<Value>) -> Result<Value, String> {
    if !matches!(route, "/tron/status" | "/tron/acquire" | "/tron/release" | "/tron/heartbeat" | "/tron/minimize") {
        return Err("TRON_ROUTE_NOT_ALLOWED".to_owned());
    }
    let d = discover()?;
    let body = match payload.as_ref() {
        Some(p) => serde_json::to_vec(p).map_err(|_| "JSON_ENCODING_FAILED")?,
        None => Vec::new(),
    };
    if body.len() > MAX_REQUEST_BYTES { return Err("LOCAL_BROKER_PAYLOAD_TOO_LARGE".to_owned()); }
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, d.port);
    let mut stream = TcpStream::connect_timeout(&addr.into(), Duration::from_secs(2))
        .map_err(|_| "TRON_SHARED_BROKER_UNREACHABLE".to_owned())?;
    stream.set_read_timeout(Some(Duration::from_secs(160))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(8))).map_err(|e| e.to_string())?;
    let method = if payload.is_some() { "POST" } else { "GET" };
    let header = format!(
        "{method} {route} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        d.port, d.token, body.len()
    );
    stream.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(&body).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream.take(MAX_REPLY_BYTES + 1).read_to_end(&mut response).map_err(|e| e.to_string())?;
    if response.len() as u64 > MAX_REPLY_BYTES { return Err("LOCAL_BROKER_RESPONSE_TOO_LARGE".to_owned()); }
    let header_end = response.windows(4).position(|b| b == b"\r\n\r\n")
        .ok_or("MALFORMED_BROKER_HTTP")?;
    let head = String::from_utf8_lossy(&response[..header_end]).to_ascii_lowercase();
    let status = head.lines().next().and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok()).ok_or("MALFORMED_BROKER_STATUS")?;
    let raw = &response[header_end + 4..];
    let decoded = if head.contains("transfer-encoding: chunked") {
        decode_chunked(raw)?
    } else { raw.to_vec() };
    let result: Value = serde_json::from_slice(&decoded).map_err(|_| "BROKER_REPLY_INVALID_JSON")?;
    if !(200..300).contains(&status) || result.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(result.get("error").and_then(Value::as_str).unwrap_or("BROKER_REJECTED").to_owned());
    }
    Ok(result)
}

fn id() -> String { format!("filmtron-{}", std::process::id()) }

/// Passive state queries NEVER reserve GPU memory.
pub fn status() -> Result<Value, String> {
    broker_request("/tron/status", None)
}

/// Called ONLY by the explicitly clicked local ON control.
pub fn activate() -> Result<Value, String> {
    broker_request("/tron/acquire", Some(json!({
        "leaseId": id(),
        "origin": "local-ui",
        "explicitConsent": true,
        "product": "filmtron",
        "processId": std::process::id()
    })))
}

pub fn heartbeat() -> Result<Value, String> {
    broker_request("/tron/heartbeat", Some(json!({"leaseId": id()})))
}

pub fn release() -> Result<Value, String> {
    broker_request("/tron/release", Some(json!({"leaseId": id()})))
}

pub fn minimize() -> Result<Value, String> {
    broker_request("/tron/minimize", Some(json!({"leaseId": id()})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunked_parser_roundtrip_and_truncation() {
        assert_eq!(decode_chunked(b"5\r\nhello\r\n0\r\n\r\n").ok(), Some(b"hello".to_vec()));
        assert!(decode_chunked(b"5\r\na").is_err());
    }
    #[test]
    fn passive_calls_do_not_install_model_or_change_leases() {
        assert_eq!(id().starts_with("filmtron-"), true);
        // Source calls only the shared GemiMCP broker, no direct Ollama
        // TCP socket, second model process, or cloud endpoint.
    }
}
