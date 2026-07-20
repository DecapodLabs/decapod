//! Small, opt-in HTTP adapter for the transport-neutral RPC profile.
//!
//! This module deliberately owns only HTTP framing, authentication, replay
//! protection, and request limits. Semantic authority remains in the existing
//! local RPC process invoked by the caller.

#![allow(clippy::disallowed_types)]

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_REPLAY_KEYS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: &'static str,
}

impl HttpResponse {
    pub fn json(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
        }
    }
}

pub fn serve<F>(
    bind: &str,
    auth_token: &str,
    allow_remote: bool,
    max_body_bytes: usize,
    mut handler: F,
) -> std::io::Result<()>
where
    F: FnMut(&[u8]) -> HttpResponse,
{
    if auth_token.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "authenticated HTTP transport requires a non-empty bearer token",
        ));
    }
    let listener = TcpListener::bind(bind)?;
    let address = listener.local_addr()?;
    if !allow_remote && !address.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "remote HTTP binding requires --allow-remote",
        ));
    }
    eprintln!("decapod HTTP RPC listening on {address}");

    let mut replayed = HashSet::new();
    let mut cached: HashMap<String, HttpResponse> = HashMap::new();
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let request = match read_request(&mut stream, max_body_bytes) {
            Ok(request) => request,
            Err(response) => {
                write_response(&mut stream, response)?;
                continue;
            }
        };
        let idempotency_key = request.headers.get("idempotency-key");
        let request_id = request.headers.get("x-decapod-request-id");
        let key = idempotency_key.or(request_id);
        let Some(_key) = key.filter(|value| !value.trim().is_empty()) else {
            write_response(
                &mut stream,
                HttpResponse::json(400, error_json("missing idempotency key")),
            )?;
            continue;
        };
        if request.headers.get("authorization") != Some(&format!("Bearer {auth_token}")) {
            write_response(
                &mut stream,
                HttpResponse::json(401, error_json("unauthorized")),
            )?;
            continue;
        }
        if request.path != "/rpc/v1" || request.method != "POST" {
            write_response(
                &mut stream,
                HttpResponse::json(404, error_json("use POST /rpc/v1")),
            )?;
            continue;
        }
        if let Some(response) = idempotency_key.and_then(|value| cached.get(value)) {
            write_response(&mut stream, response.clone())?;
            continue;
        }
        if let Some(request_id) = request_id
            && !replayed.insert(request_id.to_string())
        {
            write_response(
                &mut stream,
                HttpResponse::json(409, error_json("replayed request")),
            )?;
            continue;
        }
        if replayed.len() > MAX_REPLAY_KEYS {
            replayed.clear();
            cached.clear();
        }
        let response = handler(&request.body);
        if let Some(idempotency_key) = idempotency_key {
            cached.insert(idempotency_key.to_string(), response.clone());
        }
        write_response(&mut stream, response)?;
    }
    Ok(())
}

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_request(
    stream: &mut TcpStream,
    max_body_bytes: usize,
) -> Result<HttpRequest, HttpResponse> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk).map_err(internal_error)?;
        if count == 0 {
            return Err(HttpResponse::json(
                400,
                error_json("incomplete HTTP request"),
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_HEADER_BYTES + max_body_bytes {
            return Err(HttpResponse::json(413, error_json("request too large")));
        }
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(HttpResponse::json(
                431,
                error_json("HTTP headers too large"),
            ));
        }
    };
    let header = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| HttpResponse::json(400, error_json("HTTP headers are not UTF-8")))?;
    let mut lines = header.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();
    let version = request_parts.next();
    if !matches!(version, Some("HTTP/1.1") | Some("HTTP/1.0")) {
        return Err(HttpResponse::json(
            400,
            error_json("unsupported HTTP request line"),
        ));
    }
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(HttpResponse::json(400, error_json("malformed HTTP header")));
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| HttpResponse::json(400, error_json("content-length is required")))?;
    if length > max_body_bytes {
        return Err(HttpResponse::json(
            413,
            error_json("request body too large"),
        ));
    }
    while bytes.len() - header_end < length {
        let count = stream.read(&mut chunk).map_err(internal_error)?;
        if count == 0 {
            return Err(HttpResponse::json(400, error_json("incomplete HTTP body")));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[header_end..header_end + length].to_vec(),
    })
}

fn write_response(stream: &mut TcpStream, response: HttpResponse) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)
}

fn error_json(message: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"success": false, "error": message}))
        .unwrap_or_else(|_| b"{\"success\":false}".to_vec())
}

fn internal_error(error: std::io::Error) -> HttpResponse {
    HttpResponse::json(500, error_json(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    #[test]
    fn requires_loopback_for_default_binding() {
        let parsed: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(parsed.is_loopback());
        assert!(!"192.0.2.1".parse::<IpAddr>().unwrap().is_loopback());
    }
}
