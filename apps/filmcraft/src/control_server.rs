//! Localhost JSON-lines control server (one request per line, one reply per line). Loopback only.
//!
//! The server only ever reads requests: a line that is not a JSON object with a string `method`
//! (an HTTP request line from a browser's cross-origin `fetch`, a stray `nc`, binary junk) gets
//! one error reply and the connection is closed, so nothing after it is executed. Lines are
//! bounded ([`MAX_LINE`]) and so is the number of open connections ([`MAX_CONNECTIONS`]); junk
//! never reaches the UI thread. See `docs/control-protocol.md`.

use std::collections::hash_map::RandomState;
use std::fs::OpenOptions;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use filmcraft_ui_egui::ControlRequest;
use serde_json::{Value, json};

/// Longest accepted request line (bytes, without the newline). Requests are small JSON objects;
/// anything longer is refused and the connection closed.
pub const MAX_LINE: usize = 4 * 1024 * 1024;
/// Connections served at once; further ones get an error line and are closed.
pub const MAX_CONNECTIONS: usize = 16;
/// The first request on every TCP connection must use this method.
pub const AUTH_METHOD: &str = "auth";
/// Hard wall-clock deadline for a new connection to complete `auth`.
pub const AUTH_DEADLINE: Duration = Duration::from_secs(10);
/// Idle read/write timeout after authentication.
pub const IO_TIMEOUT: Duration = Duration::from_secs(30);

const TOKEN_HEX_LEN: usize = 64;
static TOKEN_NONCE: AtomicU64 = AtomicU64::new(1);

pub fn generate_control_token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let seq = TOKEN_NONCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let mut out = String::with_capacity(TOKEN_HEX_LEN);
    for i in 0..4u64 {
        let rs = RandomState::new();
        let mut h = rs.build_hasher();
        h.write_u64(seq.wrapping_add(i));
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        let word = h.finish();
        for b in word.to_le_bytes() {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

pub fn token_matches(expected: &str, supplied: &str) -> bool {
    if expected.len() != TOKEN_HEX_LEN || supplied.len() != TOKEN_HEX_LEN {
        return false;
    }
    if !expected.bytes().all(|b| b.is_ascii_hexdigit()) || !supplied.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    expected
        .bytes()
        .zip(supplied.bytes())
        .fold(0u8, |diff, (a, b)| diff | (a.to_ascii_lowercase() ^ b.to_ascii_lowercase()))
        == 0
}

pub fn default_port_token_file(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("filmtron-control-{port}.token"))
}

fn write_owner_only_token_file(path: &Path, token: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    writeln!(file, "{token}")?;
    drop(file);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("icacls")
            .arg(path)
            .arg("/inheritance:d")
            .arg("/remove:g")
            .args(["*S-1-1-0", "*S-1-5-32-545", "*S-1-5-11", "*S-1-5-32-546", "*S-1-5-7"])
            .arg("/q")
            .output();
    }
    Ok(())
}

fn resolve_server_token(port: u16) -> String {
    if let Some(tok) = std::env::var("FILMTRON_CONTROL_TOKEN")
        .ok()
        .or_else(|| std::env::var("FILMCRAFT_CONTROL_TOKEN").ok())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s.len() == TOKEN_HEX_LEN && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        let _ = write_owner_only_token_file(&default_port_token_file(port), &tok);
        return tok;
    }
    if let Some(file_path) = std::env::var_os("FILMTRON_CONTROL_TOKEN_FILE")
        .or_else(|| std::env::var_os("FILMCRAFT_CONTROL_TOKEN_FILE"))
        .map(PathBuf::from)
    {
        if let Ok(existing) = std::fs::read_to_string(&file_path) {
            let trimmed = existing.trim().to_ascii_lowercase();
            if trimmed.len() == TOKEN_HEX_LEN && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
                let _ = write_owner_only_token_file(&default_port_token_file(port), &trimmed);
                return trimmed;
            }
        }
        let generated = generate_control_token();
        let _ = write_owner_only_token_file(&file_path, &generated);
        let _ = write_owner_only_token_file(&default_port_token_file(port), &generated);
        return generated;
    }
    let generated = generate_control_token();
    let _ = write_owner_only_token_file(&default_port_token_file(port), &generated);
    generated
}

pub fn start(port: u16, ctx: egui::Context) -> Receiver<ControlRequest> {
    let (tx, rx) = channel::<ControlRequest>();
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("filmcraft: control server failed to bind 127.0.0.1:{port}: {e}");
            return rx;
        }
    };
    let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    let token = Arc::new(resolve_server_token(actual_port));
    eprintln!("filmcraft: control server listening on 127.0.0.1:{actual_port} (authenticated)");
    std::thread::spawn(move || accept_loop(listener, tx, ctx, token));
    rx
}

fn accept_loop(listener: TcpListener, tx: Sender<ControlRequest>, ctx: egui::Context, token: Arc<String>) {
    let open = Arc::new(AtomicUsize::new(0));
    for mut stream in listener.incoming().flatten() {
        if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::SeqCst);
            let _ = writeln!(stream, "{}", json!({"ok": false, "error": "too many control connections"}));
            continue;
        }
        let tx = tx.clone();
        let ctx = ctx.clone();
        let token = Arc::clone(&token);
        // The slot is given back when the guard drops: after `serve` returns, if it panics, or if
        // the thread can't be started (the closure, and with it the guard, is dropped).
        let slot = Slot(Arc::clone(&open));
        let spawned = std::thread::Builder::new().name("control-conn".into()).spawn(move || {
            let _slot = slot;
            serve(stream, &tx, &ctx, &token);
        });
        if let Err(e) = spawned {
            eprintln!("filmcraft: control server could not start a connection thread: {e}");
        }
    }
}

/// One counted connection; decrements the open-connection count when dropped.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn serve(stream: TcpStream, tx: &Sender<ControlRequest>, ctx: &egui::Context, expected_token: &str) {
    let _ = stream.set_read_timeout(Some(AUTH_DEADLINE));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let Ok(read) = stream.try_clone() else { return };
    let deadline = Instant::now() + AUTH_DEADLINE;
    serve_authenticated_lines(
        read,
        stream,
        expected_token,
        Some(deadline),
        |method, params| {
            let (req, rrx) = ControlRequest::new(method, params);
            tx.send(req).ok()?;
            ctx.request_repaint();
            Some(rrx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| json!({"ok": false, "error": "timeout"})))
        },
    );
}

/// One parsed line.
enum Line {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Blank,
    /// Not a request: reply with this error and close the connection.
    Reject(String),
}

fn parse_line(bytes: &[u8]) -> Line {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Line::Reject("not a control request (invalid UTF-8); closing the connection".into());
    };
    let text = text.trim();
    if text.is_empty() {
        return Line::Blank;
    }
    let msg = match serde_json::from_str::<Value>(text) {
        Ok(v) => v,
        Err(e) => return Line::Reject(format!("bad JSON ({e}); expected one JSON request object per line; closing the connection")),
    };
    let Some(obj) = msg.as_object() else {
        return Line::Reject("not a control request (expected a JSON object); closing the connection".into());
    };
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return Line::Reject("not a control request (missing string `method`); closing the connection".into());
    };
    Line::Request { id: obj.get("id").cloned().unwrap_or(Value::Null), method: method.to_string(), params: obj.get("params").cloned().unwrap_or(json!({})) }
}

/// Read one `\n`-terminated line of at most `max` bytes into `buf` (newline stripped).
/// `Ok(false)` at end of stream; `Err` when the line is too long or the read fails.
fn read_bounded_line(reader: &mut impl BufRead, buf: &mut Vec<u8>, max: usize) -> Result<bool, String> {
    buf.clear();
    // `take` bounds the bytes buffered for one line: a peer can't make us allocate more than `max`.
    let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    let n = reader.take(limit).read_until(b'\n', buf).map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(false);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        return Ok(true);
    }
    if buf.len() > max {
        return Err(format!("request line longer than {max} bytes; closing the connection"));
    }
    // Last line without a newline, then end of stream.
    Ok(true)
}

/// Require the first non-blank line on the connection to be `{"method":"auth","params":{"token":"..."}}`
/// before dispatching any request to `handle`.
pub fn serve_authenticated_lines<R: Read, W: Write>(
    read: R,
    mut out: W,
    expected_token: &str,
    auth_deadline: Option<Instant>,
    mut handle: impl FnMut(String, Value) -> Option<Value>,
) {
    let mut reader = BufReader::new(read);
    let mut buf = Vec::new();
    let mut authenticated = false;
    loop {
        if !authenticated
            && let Some(dl) = auth_deadline
            && Instant::now() >= dl
        {
            let _ = writeln!(out, "{}", json!({"ok": false, "error": "authentication timed out"})).and_then(|()| out.flush());
            return;
        }
        let reject = match read_bounded_line(&mut reader, &mut buf, MAX_LINE) {
            Ok(false) => return,
            Err(e) => e,
            Ok(true) => match parse_line(&buf) {
                Line::Blank => continue,
                Line::Reject(e) => e,
                Line::Request { id, method, params } => {
                    if !authenticated {
                        let supplied = params.get("token").and_then(Value::as_str).unwrap_or("");
                        if method == AUTH_METHOD && token_matches(expected_token, supplied) {
                            authenticated = true;
                            let reply = json!({"id": id, "ok": true, "result": {"authenticated": true}});
                            if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
                                return;
                            }
                            continue;
                        }
                        let reply = json!({"id": id, "ok": false, "error": "authentication required"});
                        let _ = writeln!(out, "{reply}").and_then(|()| out.flush());
                        return;
                    }
                    if method == AUTH_METHOD {
                        let reply = json!({"id": id, "ok": false, "error": "already authenticated"});
                        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
                            return;
                        }
                        continue;
                    }
                    let Some(mut r) = handle(method, params) else { return };
                    if let Some(o) = r.as_object_mut() {
                        o.insert("id".into(), id);
                    }
                    if writeln!(out, "{r}").and_then(|()| out.flush()).is_err() {
                        return;
                    }
                    continue;
                }
            },
        };
        let _ = writeln!(out, "{}", json!({"ok": false, "error": reject})).and_then(|()| out.flush());
        return;
    }
}

/// Serve requests from `read`, answering on `out`, until the peer closes, sends something that is
/// not a request, or `handle` returns `None` (app gone). `handle` gets `(method, params)` and
/// returns the reply object (the request's `id` is added here).
#[cfg_attr(not(test), allow(dead_code))]
fn serve_lines<R: Read, W: Write>(read: R, mut out: W, mut handle: impl FnMut(String, Value) -> Option<Value>) {
    let mut reader = BufReader::new(read);
    let mut buf = Vec::new();
    loop {
        let reject = match read_bounded_line(&mut reader, &mut buf, MAX_LINE) {
            Ok(false) => return,
            Err(e) => e,
            Ok(true) => match parse_line(&buf) {
                Line::Blank => continue,
                Line::Reject(e) => e,
                Line::Request { id, method, params } => {
                    let Some(mut r) = handle(method, params) else { return };
                    if let Some(o) = r.as_object_mut() {
                        o.insert("id".into(), id);
                    }
                    if writeln!(out, "{r}").and_then(|()| out.flush()).is_err() {
                        return;
                    }
                    continue;
                }
            },
        };
        let _ = writeln!(out, "{}", json!({"ok": false, "error": reject})).and_then(|()| out.flush());
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[u8]) -> (Vec<Value>, Vec<String>) {
        let mut out = Vec::new();
        let mut calls = Vec::new();
        serve_lines(input, &mut out, |m, _p| {
            calls.push(m);
            Some(json!({"ok": true, "result": null}))
        });
        let replies = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect();
        (replies, calls)
    }

    #[test]
    fn requests_are_answered_in_order_with_ids() {
        let (r, calls) = run(b"{\"id\":1,\"method\":\"a\"}\n\n{\"id\":2,\"method\":\"b\",\"params\":{}}\r\n{\"method\":\"c\"}");
        assert_eq!(calls, ["a", "b", "c"]);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0]["id"], 1);
        assert_eq!(r[1]["id"], 2);
        assert_eq!(r[2]["id"], Value::Null);
    }

    #[test]
    fn http_request_closes_before_the_body_runs() {
        // A browser's cross-origin `fetch` POST (issue #119): the request line is rejected and the
        // connection closed, so the JSON body is never executed.
        let http = b"POST / HTTP/1.1\r\nHost: 127.0.0.1:9876\r\nOrigin: https://attacker.example\r\nContent-Type: text/plain\r\n\r\n\
            {\"id\":7,\"method\":\"engine.execute\",\"params\":{\"command\":\"file.openDemoProject\",\"params\":{}}}\n";
        let (r, calls) = run(http);
        assert!(calls.is_empty(), "nothing may run: {calls:?}");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["ok"], false);
        assert!(r[0]["error"].as_str().unwrap().contains("closing the connection"));
    }

    #[test]
    fn non_request_json_closes_the_connection() {
        for junk in [
            &b"[1,2]\n{\"method\":\"a\"}\n"[..],
            b"{\"id\":1}\n{\"method\":\"a\"}\n",
            b"{\"method\":5}\n{\"method\":\"a\"}\n",
            b"\xff\xfe\n{\"method\":\"a\"}\n",
        ] {
            let (r, calls) = run(junk);
            assert!(calls.is_empty(), "{calls:?}");
            assert_eq!(r.len(), 1);
            assert_eq!(r[0]["ok"], false);
        }
    }

    #[test]
    fn a_request_before_junk_still_runs() {
        let (r, calls) = run(b"{\"method\":\"a\"}\nGET / HTTP/1.1\n{\"method\":\"b\"}\n");
        assert_eq!(calls, ["a"]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1]["ok"], false);
    }

    #[test]
    fn overlong_line_is_refused() {
        // One byte over the limit, then a valid request: neither runs.
        let mut input = vec![b'x'; MAX_LINE + 1];
        input.extend_from_slice(b"\n{\"method\":\"a\"}\n");
        let (r, calls) = run(&input);
        assert!(calls.is_empty(), "{calls:?}");
        assert_eq!(r.len(), 1);
        assert!(r[0]["error"].as_str().unwrap().contains("longer than"));
    }

    #[test]
    fn line_at_the_limit_is_accepted() {
        let mut buf = Vec::new();
        let line = vec![b'a'; 10];
        let mut input = line.clone();
        input.push(b'\n');
        assert_eq!(read_bounded_line(&mut &input[..], &mut buf, 10), Ok(true));
        assert_eq!(buf, line);
        assert!(read_bounded_line(&mut &[b'a'; 11][..], &mut buf, 10).is_err());
    }

    #[test]
    fn tcp_connection_is_closed_after_junk() {
        // End to end over a real loopback socket (the transport the app uses).
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (s, _) = listener.accept().unwrap();
            let read = s.try_clone().unwrap();
            serve_lines(read, s, |_, _| Some(json!({"ok": true})));
        });
        let mut c = TcpStream::connect(addr).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(b"{\"id\":7,\"method\":\"x\"}\nhello\n{\"method\":\"y\"}\n").unwrap();
        let mut text = String::new();
        c.read_to_string(&mut text).unwrap(); // returns at EOF: the server closed the connection
        server.join().unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].contains("\"id\":7"));
        assert!(lines[1].contains("closing the connection"));
    }

    #[test]
    fn extra_connections_are_refused() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, _rx) = channel::<ControlRequest>();
        let token = Arc::new(generate_control_token());
        std::thread::spawn(move || accept_loop(listener, tx, egui::Context::default(), token));
        // Idle connections are accepted in order and held open.
        let held: Vec<TcpStream> = (0..16).map(|_| TcpStream::connect(addr).unwrap()).collect();
        let mut extra = TcpStream::connect(addr).unwrap();
        extra.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut line = String::new();
        BufReader::new(&mut extra).read_line(&mut line).unwrap();
        assert!(line.contains("too many control connections"), "{line}");
        drop(held);
    }

    #[test]
    fn authenticated_server_rejects_unauthenticated_and_accepts_valid_token() {
        let token = generate_control_token();
        // 1. Unauthenticated request is rejected before handle runs.
        let mut out = Vec::new();
        let mut calls = Vec::new();
        serve_authenticated_lines(&b"{\"id\":1,\"method\":\"ui.inspect\"}\n"[..], &mut out, &token, None, |m, _| {
            calls.push(m);
            Some(json!({"ok": true}))
        });
        assert!(calls.is_empty(), "unauthenticated request must not run");
        let reply: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], "authentication required");

        // 2. Wrong token is rejected before handle runs.
        out.clear();
        let wrong = format!("{{\"id\":1,\"method\":\"auth\",\"params\":{{\"token\":\"{}\"}}}}\n{{\"id\":2,\"method\":\"ui.inspect\"}}\n", "0".repeat(64));
        serve_authenticated_lines(wrong.as_bytes(), &mut out, &token, None, |m, _| {
            calls.push(m);
            Some(json!({"ok": true}))
        });
        assert!(calls.is_empty(), "wrong token must not run");
        let reply: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], "authentication required");

        // 3. Valid token authenticates and dispatches subsequent methods; repeat auth returns error.
        out.clear();
        let valid = format!(
            "{{\"id\":0,\"method\":\"auth\",\"params\":{{\"token\":\"{token}\"}}}}\n{{\"id\":1,\"method\":\"ui.inspect\"}}\n{{\"id\":2,\"method\":\"auth\",\"params\":{{\"token\":\"{token}\"}}}}\n"
        );
        serve_authenticated_lines(valid.as_bytes(), &mut out, &token, None, |m, _| {
            calls.push(m);
            Some(json!({"ok": true, "result": {"seen": true}}))
        });
        assert_eq!(calls, ["ui.inspect"]);
        let replies: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0]["result"]["authenticated"], true);
        assert_eq!(replies[1]["result"]["seen"], true);
        assert_eq!(replies[2]["error"], "already authenticated");
    }
}
