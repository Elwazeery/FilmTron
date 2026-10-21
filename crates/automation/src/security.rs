//! Shared security primitives for FilmTron's loopback JSON-lines control server, bridge client,
//! workspace path confinement (`AuthorizedWorkspace`), optimistic concurrency (`expectedRevision`),
//! and idempotent command replay (`idempotencyKey`).

use std::collections::VecDeque;
use std::collections::hash_map::RandomState;
use std::fs::OpenOptions;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::AutomationError;

/// The first request on every TCP connection must use this method.
pub const AUTH_METHOD: &str = "auth";
/// Maximum encoded JSON request line, including its newline (1 MiB).
pub const MAX_REQUEST_BYTES: usize = 1 << 20;
/// Maximum simultaneously serviced TCP connections per listener.
pub const MAX_CONNECTIONS: usize = 16;
/// Maximum commands or method calls in one batch.
pub const MAX_BATCH_STEPS: usize = 256;
/// Idle/read and write timeout for loopback TCP connections.
pub const IO_TIMEOUT: Duration = Duration::from_secs(30);
/// Hard wall-clock deadline for a new TCP connection to complete `auth`.
pub const AUTH_DEADLINE: Duration = Duration::from_secs(10);
/// Maximum cached idempotency keys per session.
pub const MAX_IDEMPOTENCY_ENTRIES: usize = 256;

const TOKEN_BYTES: usize = 32;
const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;

static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Pure-Rust SHA-256 (FIPS 180-4) used for CSPRNG entropy whitening, OIDC PKCE `S256`, and
/// request parameter fingerprinting.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity((data.len() + 9).div_ceil(64) * 64);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Format bytes as lowercase hexadecimal.
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Result of reading one bounded JSON-lines frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRead {
    Eof,
    Line,
    TooLong,
}

/// Read one line without ever buffering more than [`MAX_REQUEST_BYTES`] plus one byte.
pub fn read_bounded_line(reader: &mut impl BufRead, line: &mut String) -> std::io::Result<LineRead> {
    line.clear();
    let mut bytes = Vec::new();
    let n = std::io::Read::take(reader, (MAX_REQUEST_BYTES + 1) as u64).read_until(b'\n', &mut bytes)?;
    if n == 0 {
        Ok(LineRead::Eof)
    } else if n > MAX_REQUEST_BYTES {
        line.push_str(&String::from_utf8_lossy(&bytes));
        Ok(LineRead::TooLong)
    } else {
        *line = String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(LineRead::Line)
    }
}

/// Skip the rest of the current line (through its newline, or to EOF) without buffering it.
pub fn discard_rest_of_line(reader: &mut impl BufRead) -> std::io::Result<()> {
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(());
        }
        if let Some(end) = buf.iter().position(|&b| b == b'\n') {
            reader.consume(end + 1);
            return Ok(());
        }
        let n = buf.len();
        reader.consume(n);
    }
}

/// Generate a 256-bit bearer token seeded from the operating system CSPRNG (`RandomState` uses
/// `BCryptGenRandom` on Windows and `getrandom` on Unix) mixed through SHA-256.
pub fn generate_token() -> Result<String, AutomationError> {
    let mut seed = Vec::with_capacity(160);
    let seq = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    seed.extend_from_slice(&seq.to_le_bytes());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    let now_nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    seed.extend_from_slice(&now_nanos.to_le_bytes());
    for i in 0..8u64 {
        let rs = RandomState::new();
        let mut hasher = rs.build_hasher();
        hasher.write_u64(seq.wrapping_add(i));
        hasher.write_u128(now_nanos);
        seed.extend_from_slice(&hasher.finish().to_le_bytes());
    }
    #[cfg(unix)]
    if let Ok(mut urandom) = std::fs::File::open("/dev/urandom") {
        let mut os_bytes = [0u8; 32];
        if urandom.read_exact(&mut os_bytes).is_ok() {
            seed.extend_from_slice(&os_bytes);
        }
    }
    let digest = sha256(&seed);
    Ok(hex_encode(&digest))
}

/// Accept only the fixed-width 64-char hexadecimal representation emitted by [`generate_token`].
pub fn validate_token(token: &str) -> Result<(), AutomationError> {
    if token.len() != TOKEN_HEX_LEN || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AutomationError::BadRequest("control token must contain exactly 64 hexadecimal characters".into()));
    }
    Ok(())
}

/// Constant-time comparison of fixed-width 64-hex tokens (case-insensitive for `A`–`F`).
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
        .fold(0u8, |different, (a, b)| different | (a.to_ascii_lowercase() ^ b.to_ascii_lowercase()))
        == 0
}

/// Validate the first TCP frame without exposing any control method before authentication.
pub fn authentication_reply(line: &str, expected_token: &str) -> (Value, bool) {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return (json!({"id": null, "ok": false, "error": "authentication required"}), false);
        }
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let supplied = req.get("params").and_then(|p| p.get("token")).and_then(Value::as_str).unwrap_or("");
    let ok = req.get("method").and_then(Value::as_str) == Some(AUTH_METHOD) && token_matches(expected_token, supplied);
    if ok {
        (json!({"id": id, "ok": true, "result": {"authenticated": true}}), true)
    } else {
        (json!({"id": id, "ok": false, "error": "authentication required"}), false)
    }
}

/// Answer a repeat `auth` frame on an already authenticated connection.
pub fn repeat_auth_reply(line: &str) -> Option<Value> {
    let req: Value = serde_json::from_str(line).ok()?;
    if req.get("method").and_then(Value::as_str) != Some(AUTH_METHOD) {
        return None;
    }
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    Some(json!({"id": id, "ok": false, "error": "already authenticated"}))
}

/// Verify that a bind or bridge address targets the local loopback interface only.
pub fn validate_loopback_bind_addr(addr: &str) -> Result<(), AutomationError> {
    let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(addr);
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
        return Err(AutomationError::BadRequest(format!("control address must be loopback (127.0.0.1 / ::1), got `{addr}`")));
    }
    Ok(())
}

/// Read and validate an existing owner-only control token file.
pub fn read_token_file(path: &Path) -> Result<String, AutomationError> {
    let mut file = std::fs::File::open(path).map_err(|e| AutomationError::Other(format!("{}: {e}", path.display())))?;
    let meta = file.metadata().map_err(|e| AutomationError::Other(format!("{}: {e}", path.display())))?;
    if !meta.is_file() {
        return Err(AutomationError::Other(format!("{}: control token file is not a regular file", path.display())));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o077 != 0 {
            return Err(AutomationError::Other(format!(
                "{}: control token file is readable by group or others (mode {:o}); restrict it to its owner (chmod 600)",
                path.display(),
                meta.mode() & 0o777
            )));
        }
    }
    #[cfg(windows)]
    {
        verify_windows_token_acl(path)?;
    }
    let token = std::io::read_to_string(&mut file).map_err(|e| AutomationError::Other(format!("{}: {e}", path.display())))?;
    let token = token.trim().to_owned();
    validate_token(&token)?;
    Ok(token)
}

#[cfg(windows)]
pub fn restrict_windows_token_acl(path: &Path) -> Result<(), AutomationError> {
    let output = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:d")
        .arg("/remove:g")
        .args(["*S-1-1-0", "*S-1-5-32-545", "*S-1-5-11", "*S-1-5-32-546", "*S-1-5-7"])
        .arg("/q")
        .output()
        .map_err(|e| AutomationError::Other(format!("{}: cannot invoke icacls: {e}", path.display())))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AutomationError::Other(format!("{}: cannot restrict token file ACL: {}", path.display(), stderr.trim())));
    }
    Ok(())
}

#[cfg(windows)]
pub fn verify_windows_token_acl(path: &Path) -> Result<(), AutomationError> {
    let nonce = generate_token()?;
    let acl_path = std::env::temp_dir().join(format!("filmtron-acl-{}-{nonce}.sddl", std::process::id()));
    let output = std::process::Command::new("icacls")
        .arg(path)
        .arg("/save")
        .arg(&acl_path)
        .arg("/q")
        .output()
        .map_err(|e| AutomationError::Other(format!("{}: cannot inspect token file ACL: {e}", path.display())))?;
    let bytes = std::fs::read(&acl_path).unwrap_or_default();
    let _ = std::fs::remove_file(&acl_path);
    if !output.status.success() || bytes.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AutomationError::Other(format!("{}: cannot verify token file ACL: {}", path.display(), stderr.trim())));
    }
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let sddl = String::from_utf16_lossy(&units);
    for ace in sddl.split('(').filter(|part| part.starts_with("A;")) {
        for forbidden in [";WD)", ";BU)", ";AU)", ";BG)", ";AN)"] {
            if ace.contains(forbidden) {
                return Err(AutomationError::Other(format!(
                    "{}: control token file is readable by other local principals ({forbidden}); restrict it to its owner",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

/// Create or overwrite a token file with owner-only permissions (`0600` on Unix, restricted ACL on Windows).
pub fn create_token_file(path: &Path, token: &str) -> Result<(), AutomationError> {
    validate_token(token)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| AutomationError::Other(format!("{}: {e}", parent.display())))?;
    }
    let _ = std::fs::remove_file(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| AutomationError::Other(format!("{}: {e}", path.display())))?;
    writeln!(file, "{token}").map_err(|e| AutomationError::Other(format!("{}: {e}", path.display())))?;
    drop(file);
    #[cfg(windows)]
    if let Err(e) = restrict_windows_token_acl(path) {
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    Ok(())
}

/// Default owner-only token file path for a given control port.
pub fn default_port_token_file(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("filmtron-control-{port}.token"))
}

/// Resolve a server token. With no supplied token or file, a fresh token is returned.
/// A missing token file is created atomically; an existing one is read and validated.
pub fn server_token(supplied: Option<&str>, token_file: Option<&Path>) -> Result<String, AutomationError> {
    if supplied.is_some() && token_file.is_some() {
        return Err(AutomationError::BadRequest("use either a control token or a control token file, not both".into()));
    }
    if let Some(token) = supplied {
        validate_token(token)?;
        return Ok(token.to_ascii_lowercase());
    }
    let token = generate_token()?;
    let Some(path) = token_file else {
        return Ok(token);
    };
    match read_token_file(path) {
        Ok(existing) => Ok(existing.to_ascii_lowercase()),
        Err(_) if !path.exists() => {
            create_token_file(path, &token)?;
            Ok(token)
        }
        Err(e) => Err(e),
    }
}

/// Resolve the token used by a client. Clients never silently generate credentials.
pub fn client_token(supplied: Option<&str>, token_file: Option<&Path>) -> Result<String, AutomationError> {
    if supplied.is_some() && token_file.is_some() {
        return Err(AutomationError::BadRequest("use either a control token or a control token file, not both".into()));
    }
    if let Some(token) = supplied {
        validate_token(token)?;
        return Ok(token.to_ascii_lowercase());
    }
    token_file.map_or_else(
        || {
            Err(AutomationError::BadRequest(
                "bridge mode needs --control-token, --control-token-file, FILMTRON_CONTROL_TOKEN, or FILMTRON_CONTROL_TOKEN_FILE".into(),
            ))
        },
        |path| read_token_file(path).map(|v| v.to_ascii_lowercase()),
    )
}

/// Environment-aware token inputs shared by the desktop app and CLI.
pub fn token_inputs(supplied: Option<String>, token_file: Option<PathBuf>) -> (Option<String>, Option<PathBuf>) {
    let supplied = supplied
        .or_else(|| std::env::var("FILMTRON_CONTROL_TOKEN").ok().filter(|s| !s.trim().is_empty()))
        .or_else(|| std::env::var("FILMCRAFT_CONTROL_TOKEN").ok().filter(|s| !s.trim().is_empty()));
    let token_file = token_file
        .or_else(|| std::env::var_os("FILMTRON_CONTROL_TOKEN_FILE").filter(|s| !s.is_empty()).map(PathBuf::from))
        .or_else(|| std::env::var_os("FILMCRAFT_CONTROL_TOKEN_FILE").filter(|s| !s.is_empty()).map(PathBuf::from));
    (supplied, token_file)
}

/// Resolve an optional client token for a bridge address (`127.0.0.1:<port>`): checks explicit
/// token, environment variables, and finally the owner-only `filmtron-control-<port>.token` file
/// written by `filmcraft --control <port>`.
pub fn resolve_bridge_token(addr: &str, explicit_token: Option<&str>) -> Result<Option<String>, AutomationError> {
    if let Some(tok) = explicit_token {
        validate_token(tok)?;
        return Ok(Some(tok.to_ascii_lowercase()));
    }
    let (env_tok, env_file) = token_inputs(None, None);
    if env_tok.is_some() || env_file.is_some() {
        return client_token(env_tok.as_deref(), env_file.as_deref()).map(Some);
    }
    if let Some(port) = addr.rsplit_once(':').and_then(|(_, p)| p.parse::<u16>().ok()) {
        let default_file = default_port_token_file(port);
        if default_file.exists() {
            return read_token_file(&default_file).map(|t| Some(t.to_ascii_lowercase()));
        }
    }
    Ok(None)
}

/// Counts active connections and returns a permit only while below the configured maximum.
pub struct ConnectionLimiter {
    active: AtomicUsize,
    max: usize,
}

impl ConnectionLimiter {
    pub fn new(max: usize) -> Arc<Self> {
        Arc::new(Self { active: AtomicUsize::new(0), max })
    }

    pub fn try_acquire(self: &Arc<Self>) -> Option<ConnectionPermit> {
        let mut current = self.active.load(Ordering::Acquire);
        loop {
            if current >= self.max {
                return None;
            }
            match self.active.compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(ConnectionPermit { limiter: Arc::clone(self) }),
                Err(actual) => current = actual,
            }
        }
    }
}

pub struct ConnectionPermit {
    limiter: Arc<ConnectionLimiter>,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.limiter.active.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Apply idle and write timeouts before handing a socket to a connection worker.
pub fn configure_stream(stream: &std::net::TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))
}

/// The read half of a control connection. Until [`Self::authenticated`] is called, reads fail
/// with `TimedOut` once the authentication deadline has passed, whatever arrived in the meantime.
pub struct AuthDeadlineStream {
    stream: std::net::TcpStream,
    deadline: Option<Instant>,
}

impl AuthDeadlineStream {
    pub fn new(stream: std::net::TcpStream, limit: Duration) -> Self {
        let now = Instant::now();
        Self { stream, deadline: Some(now.checked_add(limit).unwrap_or(now)) }
    }

    pub fn authenticated(&mut self) -> std::io::Result<()> {
        self.deadline = None;
        self.stream.set_read_timeout(Some(IO_TIMEOUT))
    }
}

impl Read for AuthDeadlineStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(deadline) = self.deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "not authenticated in time"));
            }
            self.stream.set_read_timeout(Some(left.min(IO_TIMEOUT)))?;
        }
        self.stream.read(buf)
    }
}

/// Explicit project and media-root path confinement policy (`FILM-010`).
#[derive(Clone, Debug, Default)]
pub struct AuthorizedWorkspace {
    roots: Vec<PathBuf>,
}

impl AuthorizedWorkspace {
    /// Construct a workspace policy confining file access to the given root directories.
    pub fn new(roots: impl IntoIterator<Item = impl AsRef<Path>>) -> Result<Self, AutomationError> {
        let mut resolved = Vec::new();
        for root in roots {
            let p = root.as_ref();
            if p.as_os_str().is_empty() {
                return Err(AutomationError::BadRequest("authorized workspace root cannot be empty".into()));
            }
            let abs = std::path::absolute(p).map_err(|e| AutomationError::Other(format!("cannot resolve root `{}`: {e}", p.display())))?;
            let canon = std::fs::canonicalize(&abs).unwrap_or(abs);
            resolved.push(canon);
        }
        Ok(Self { roots: resolved })
    }

    /// Read optional workspace roots from `FILMTRON_ALLOWED_ROOTS` or `FILMCRAFT_ALLOWED_ROOTS`
    /// (separated by `;` on Windows or `:` on Unix).
    pub fn from_env() -> Result<Option<Self>, AutomationError> {
        let raw = std::env::var("FILMTRON_ALLOWED_ROOTS")
            .ok()
            .or_else(|| std::env::var("FILMCRAFT_ALLOWED_ROOTS").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let Some(raw) = raw else { return Ok(None) };
        let sep = if cfg!(windows) { ';' } else { ':' };
        let roots: Vec<PathBuf> = raw.split(sep).map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from).collect();
        if roots.is_empty() {
            return Ok(None);
        }
        Self::new(roots).map(Some)
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Validate a file path against traversal, NUL bytes, Windows reserved device names, and the
    /// configured workspace roots.
    pub fn authorize_path(&self, raw: &str) -> Result<PathBuf, AutomationError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(AutomationError::BadRequest("file path is empty".into()));
        }
        if trimmed.contains('\0') {
            return Err(AutomationError::BadRequest("file path contains NUL byte".into()));
        }
        let path = Path::new(trimmed);
        for comp in path.components() {
            match comp {
                Component::ParentDir => {
                    return Err(AutomationError::BadRequest(format!("path traversal (`..`) is denied: `{raw}`")));
                }
                Component::Normal(seg) => {
                    let s = seg.to_string_lossy();
                    let stem = s.split('.').next().unwrap_or("");
                    if is_windows_device_name(stem) {
                        return Err(AutomationError::BadRequest(format!("reserved device name is denied: `{raw}`")));
                    }
                }
                _ => {}
            }
        }
        if self.roots.is_empty() {
            return Err(AutomationError::BadRequest("automation filesystem access is not granted: no authorized workspace roots configured".into()));
        }
        let candidate = if path.is_absolute() {
            std::path::absolute(path).map_err(|e| AutomationError::Other(format!("{raw}: {e}")))?
        } else {
            self.roots[0].join(path)
        };
        let resolved = std::fs::canonicalize(&candidate).unwrap_or_else(|_| {
            if let Some(parent) = candidate.parent()
                && let Ok(canon_parent) = std::fs::canonicalize(parent)
                && let Some(name) = candidate.file_name()
            {
                return canon_parent.join(name);
            }
            candidate
        });
        let allowed = self.roots.iter().any(|root| path_starts_with_ci(&resolved, root));
        if !allowed {
            return Err(AutomationError::BadRequest(format!(
                "ACCESS_DENIED: path `{raw}` is outside authorized workspace roots"
            )));
        }
        Ok(resolved)
    }

    /// Inspect an engine command's parameters and verify all file/directory paths lie within the
    /// authorized workspace roots.
    pub fn authorize_command(&self, _cmd: &str, params: &Value) -> Result<(), AutomationError> {
        let Some(obj) = params.as_object() else { return Ok(()) };
        for key in ["path", "file", "output", "directory", "location", "databaseLocation"] {
            if let Some(p) = obj.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                self.authorize_path(p)?;
            }
        }
        if let Some(paths) = obj.get("paths").and_then(Value::as_array) {
            for item in paths {
                if let Some(p) = item.as_str() {
                    self.authorize_path(p)?;
                }
            }
        }
        Ok(())
    }
}

fn is_windows_device_name(stem: &str) -> bool {
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn path_starts_with_ci(path: &Path, root: &Path) -> bool {
    let mut p_comps = path.components();
    for r_comp in root.components() {
        let Some(p_comp) = p_comps.next() else { return false };
        let same = if cfg!(windows) {
            p_comp.as_os_str().to_string_lossy().eq_ignore_ascii_case(&r_comp.as_os_str().to_string_lossy())
        } else {
            p_comp == r_comp
        };
        if !same {
            return false;
        }
    }
    true
}

/// Bounded idempotency cache and optimistic concurrency (`expectedRevision`) coordinator (`FILM-003`).
#[derive(Default)]
pub struct IdempotencyCache {
    entries: VecDeque<(String, String, [u8; 32], Value)>,
}

impl IdempotencyCache {
    pub fn new() -> Self {
        Self { entries: VecDeque::with_capacity(MAX_IDEMPOTENCY_ENTRIES) }
    }

    /// Look up a cached result for `(cmd, idempotency_key)` and verify parameter fingerprint.
    pub fn check(&self, cmd: &str, key: &str, params_hash: &[u8; 32]) -> Result<Option<Value>, AutomationError> {
        for (c, k, h, val) in &self.entries {
            if c == cmd && k == key {
                if h != params_hash {
                    return Err(AutomationError::BadRequest(format!(
                        "IDEMPOTENCY_CONFLICT: idempotencyKey `{key}` for `{cmd}` was already used with different parameters"
                    )));
                }
                return Ok(Some(val.clone()));
            }
        }
        Ok(None)
    }

    /// Record a completed command execution under `(cmd, idempotency_key)`.
    pub fn insert(&mut self, cmd: String, key: String, params_hash: [u8; 32], result: Value) {
        if self.entries.len() >= MAX_IDEMPOTENCY_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back((cmd, key, params_hash, result));
    }
}

/// Hash a JSON parameter object (excluding `expectedRevision` and `idempotencyKey`) for idempotency comparison.
pub fn fingerprint_params(params: &Value) -> [u8; 32] {
    let mut cleaned = params.clone();
    if let Some(obj) = cleaned.as_object_mut() {
        obj.remove("expectedRevision");
        obj.remove("idempotencyKey");
    }
    let canonical = serde_json::to_vec(&cleaned).unwrap_or_default();
    sha256(&canonical)
}

/// Execute an engine command on `session` with optional `expectedRevision` optimistic concurrency
/// fence and `idempotencyKey` replay protection.
pub fn execute_guarded(
    session: &mut filmcraft_engine::Session,
    cache: &mut IdempotencyCache,
    workspace: Option<&AuthorizedWorkspace>,
    cmd: &str,
    mut params: Value,
) -> Result<Value, AutomationError> {
    if let Some(ws) = workspace {
        ws.authorize_command(cmd, &params)?;
    }

    let expected_revision = params.get("expectedRevision").and_then(Value::as_u64);
    let idempotency_key = params.get("idempotencyKey").and_then(Value::as_str).map(str::to_string);
    let has_guards = expected_revision.is_some() || idempotency_key.is_some();

    let params_hash = fingerprint_params(&params);
    if let Some(key) = &idempotency_key
        && let Some(mut cached) = cache.check(cmd, key, &params_hash)?
    {
        if let Some(obj) = cached.as_object_mut() {
            obj.insert("idempotentReplay".into(), json!(true));
            obj.insert("revision".into(), json!(session.revision));
        }
        return Ok(cached);
    }

    if let Some(expected) = expected_revision
        && session.revision != expected
    {
        return Err(AutomationError::BadRequest(format!(
            "REVISION_CONFLICT: expected revision {expected}, actual revision {}",
            session.revision
        )));
    }

    if let Some(obj) = params.as_object_mut() {
        obj.remove("expectedRevision");
        obj.remove("idempotencyKey");
    }

    let mut result = session.execute(cmd, params)?;
    if has_guards {
        if let Some(obj) = result.as_object_mut() {
            obj.entry("revision").or_insert_with(|| json!(session.revision));
            obj.entry("idempotentReplay").or_insert_with(|| json!(false));
        } else {
            result = json!({
                "value": result,
                "revision": session.revision,
                "idempotentReplay": false,
            });
        }
    }
    if let Some(key) = idempotency_key {
        cache.insert(cmd.to_string(), key, params_hash, result.clone());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_valid_and_distinct() {
        let a = generate_token().unwrap();
        let b = generate_token().unwrap();
        validate_token(&a).unwrap();
        validate_token(&b).unwrap();
        assert_ne!(a, b);
        assert!(token_matches(&a, &a));
        assert!(!token_matches(&a, &b));
        assert!(!token_matches(&a, "short"));
    }

    #[test]
    fn sha256_matches_nist_vector() {
        let digest = hex_encode(&sha256(b"abc"));
        assert_eq!(digest, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn bounded_reader_rejects_an_oversized_line() {
        let input = format!("{}\n", "x".repeat(MAX_REQUEST_BYTES + 1));
        let mut reader = std::io::Cursor::new(input);
        let mut line = String::new();
        assert_eq!(read_bounded_line(&mut reader, &mut line).unwrap(), LineRead::TooLong);
        assert_eq!(line.len(), MAX_REQUEST_BYTES + 1);
        discard_rest_of_line(&mut reader).unwrap();
        assert_eq!(read_bounded_line(&mut reader, &mut line).unwrap(), LineRead::Eof);
    }

    #[test]
    fn authentication_does_not_dispatch_without_the_token() {
        let token = generate_token().unwrap();
        let (reply, authenticated) = authentication_reply(r#"{"id":1,"method":"methods","params":{}}"#, &token);
        assert!(!authenticated);
        assert_eq!(reply["error"], "authentication required");
        assert!(reply.get("result").is_none());

        let line = json!({"id": 2, "method": AUTH_METHOD, "params": {"token": token}}).to_string();
        let (reply, authenticated) = authentication_reply(&line, &token);
        assert!(authenticated);
        assert_eq!(reply["result"]["authenticated"], true);
    }

    #[test]
    fn token_authenticates_case_insensitively_and_rejects_non_hex() {
        let configured = "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";
        let expected = server_token(Some(configured), None).unwrap();
        let line = json!({"id": 1, "method": AUTH_METHOD, "params": {"token": configured}}).to_string();
        let (reply, authenticated) = authentication_reply(&line, &expected);
        assert!(authenticated, "{reply}");
        let not_hex = "0123456789ABCDEG0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";
        assert!(!token_matches(&expected, not_hex));
    }

    #[test]
    fn repeat_auth_on_an_authenticated_connection_says_so() {
        let line = json!({"id": 3, "method": AUTH_METHOD, "params": {"token": "anything"}}).to_string();
        let reply = repeat_auth_reply(&line).unwrap();
        assert_eq!(reply["id"], 3);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], "already authenticated");
        assert!(repeat_auth_reply(r#"{"id":4,"method":"methods"}"#).is_none());
    }

    #[test]
    fn loopback_bind_address_validation_rejects_public_interfaces() {
        assert!(validate_loopback_bind_addr("127.0.0.1:9876").is_ok());
        assert!(validate_loopback_bind_addr("localhost:9876").is_ok());
        assert!(validate_loopback_bind_addr("[::1]:9876").is_ok());
        assert!(validate_loopback_bind_addr("0.0.0.0:9876").is_err());
        assert!(validate_loopback_bind_addr("192.168.1.10:9876").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn an_existing_token_file_must_be_owner_only() {
        let dir = std::env::temp_dir().join(format!("filmtron-token-perm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("token.txt");
        let token = "0123456789abcdef".repeat(4);
        std::fs::write(&path, &token).unwrap();

        let grant = std::process::Command::new("icacls").arg(&path).args(["/grant", "*S-1-1-0:(R)", "/q"]).output().unwrap();
        assert!(grant.status.success());
        assert!(read_token_file(&path).is_err(), "a world-readable Windows token file must be refused");

        restrict_windows_token_acl(&path).unwrap();
        assert_eq!(read_token_file(&path).unwrap(), token, "an owner-only Windows token file must be read");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn authorized_workspace_confines_paths_and_rejects_traversal_and_devices() {
        let dir = std::env::temp_dir().join(format!("filmtron-ws-{}", std::process::id()));
        let allowed = dir.join("project");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let ws = AuthorizedWorkspace::new([&allowed]).unwrap();
        let ok_file = allowed.join("cut.fcproj");
        assert!(ws.authorize_path(&ok_file.to_string_lossy()).is_ok());
        assert!(ws.authorize_path("clips/a.mp4").is_ok());

        let bad_outside = outside.join("secret.fcproj");
        assert!(ws.authorize_path(&bad_outside.to_string_lossy()).is_err());
        assert!(ws.authorize_path("../outside/secret.fcproj").is_err());
        assert!(ws.authorize_path("CON.mp4").is_err());
        assert!(ws.authorize_path("NUL").is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn revision_fence_and_idempotency_cache_prevent_stale_and_duplicate_mutations() {
        let mut session = filmcraft_engine::Session::default();
        session.execute("file.openDemoProject", json!({})).unwrap();
        let mut cache = IdempotencyCache::new();
        let rev0 = session.revision;

        // Wrong expectedRevision is rejected without mutating session.revision.
        let err = execute_guarded(
            &mut session,
            &mut cache,
            None,
            "sequence.addEditAllTracks",
            json!({"seconds": 2.0, "expectedRevision": rev0 + 99}),
        )
        .unwrap_err();
        assert!(err.to_string().contains("REVISION_CONFLICT"), "{err}");
        assert_eq!(session.revision, rev0);

        // Matching expectedRevision + idempotencyKey executes once and increments revision.
        let r1 = execute_guarded(
            &mut session,
            &mut cache,
            None,
            "sequence.addEditAllTracks",
            json!({"seconds": 2.0, "expectedRevision": rev0, "idempotencyKey": "cut-2s"}),
        )
        .unwrap();
        let rev1 = session.revision;
        assert!(rev1 > rev0);
        assert_eq!(r1["idempotentReplay"], json!(false));
        assert_eq!(r1["revision"], json!(rev1));

        // Replaying the same idempotencyKey with the same parameters returns the cached result
        // without re-executing or changing revision, even though session.revision is now rev1.
        let r2 = execute_guarded(
            &mut session,
            &mut cache,
            None,
            "sequence.addEditAllTracks",
            json!({"seconds": 2.0, "expectedRevision": rev0, "idempotencyKey": "cut-2s"}),
        )
        .unwrap();
        assert_eq!(session.revision, rev1);
        assert_eq!(r2["idempotentReplay"], json!(true));
        assert_eq!(r2["cuts"], r1["cuts"]);

        // Reusing the same idempotencyKey with different parameters is rejected.
        let conflict = execute_guarded(
            &mut session,
            &mut cache,
            None,
            "sequence.addEditAllTracks",
            json!({"seconds": 4.0, "idempotencyKey": "cut-2s"}),
        )
        .unwrap_err();
        assert!(conflict.to_string().contains("IDEMPOTENCY_CONFLICT"), "{conflict}");
    }
}
