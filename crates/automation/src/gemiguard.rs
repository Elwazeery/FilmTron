//! Gemi Access (`auth.gemitron.net`) OIDC PKCE (`S256`) consumer contracts and GemiGuard
//! (`gemiguard.offline-lease.v1`, `gemiguard.revocation-list.v1`) RFC 8032 Ed25519 verification
//! (`FILM-006`).
//!
//! Security & product invariants:
//! - FilmTron does not implement a parallel identity or billing service; it consumes
//!   `https://auth.gemitron.net` OIDC PKCE tokens and `GemiGuard` signed offline leases.
//! - Offline lease expiration or device/lease revocation immediately disables cloud GemiMCP
//!   federation (`cloud_mcp_allowed: false`) while strictly preserving local native `.fcproj`
//!   editing and saving (`local_editing_allowed: true`, `preserve_user_project_data: true`).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::security::{generate_token, hex_encode, sha256};

pub const GEMI_ACCESS_ISSUER: &str = "https://auth.gemitron.net";
pub const GEMI_ACCESS_CLIENT_ID: &str = "filmtron-desktop";
pub const DEFAULT_PKCE_REDIRECT_URI: &str = "http://127.0.0.1:49215/oauth/callback";
pub const OFFLINE_LEASE_SCHEMA: &str = "gemiguard.offline-lease.v1";
pub const REVOCATION_LIST_SCHEMA: &str = "gemiguard.revocation-list.v1";
pub const PRODUCT_ID: &str = "filmtron";

/// OIDC PKCE (`S256`, RFC 7636) verifier, challenge, and anti-CSRF state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub state: String,
}

impl PkceChallenge {
    /// Generate a cryptographically random PKCE `code_verifier` and its `S256` `code_challenge`.
    pub fn generate() -> Result<Self, String> {
        let t1 = generate_token().map_err(|e| e.to_string())?;
        let t2 = generate_token().map_err(|e| e.to_string())?;
        let raw_verifier = format!("{t1}{t2}");
        let verifier_bytes = raw_verifier.as_bytes();
        let verifier = URL_SAFE_NO_PAD.encode(&verifier_bytes[..48]);
        let challenge = Self::compute_s256_challenge(&verifier);
        let state = generate_token().map_err(|e| e.to_string())?;
        Ok(Self {
            code_verifier: verifier,
            code_challenge: challenge,
            code_challenge_method: "S256".into(),
            state,
        })
    }

    /// Compute RFC 7636 `BASE64URL-NO-PAD(SHA256(ASCII(code_verifier)))`.
    pub fn compute_s256_challenge(code_verifier: &str) -> String {
        let digest = sha256(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }

    /// Verify that `code_verifier` matches `code_challenge`.
    pub fn verify(&self, verifier: &str) -> bool {
        self.code_challenge_method == "S256" && Self::compute_s256_challenge(verifier) == self.code_challenge
    }
}

/// Browser-based OIDC PKCE authorization request targeting `https://auth.gemitron.net`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcAuthorizationRequest {
    pub issuer: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub state: String,
}

impl OidcAuthorizationRequest {
    pub fn new(pkce: &PkceChallenge, redirect_uri: Option<&str>) -> Self {
        Self {
            issuer: GEMI_ACCESS_ISSUER.into(),
            client_id: GEMI_ACCESS_CLIENT_ID.into(),
            redirect_uri: redirect_uri.unwrap_or(DEFAULT_PKCE_REDIRECT_URI).into(),
            scopes: vec![
                "openid".into(),
                "profile".into(),
                "filmtron:read".into(),
                "filmtron:edit".into(),
                "filmtron:render".into(),
                "filmtron:ui".into(),
            ],
            code_challenge: pkce.code_challenge.clone(),
            code_challenge_method: pkce.code_challenge_method.clone(),
            state: pkce.state.clone(),
        }
    }

    pub fn authorization_url(&self) -> String {
        let scope_joined = self.scopes.join("%20");
        format!(
            "{}/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method={}&state={}",
            self.issuer.trim_end_matches('/'),
            url_encode(&self.client_id),
            url_encode(&self.redirect_uri),
            scope_joined,
            url_encode(&self.code_challenge),
            url_encode(&self.code_challenge_method),
            url_encode(&self.state),
        )
    }
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&hex_encode(&[b]).to_ascii_uppercase());
        }
    }
    out
}

/// Claims extracted from a Gemi Access (`auth.gemitron.net`) access token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GemiAccessClaims {
    pub iss: String,
    pub sub: String,
    pub aud: Vec<String>,
    pub exp: u64,
    pub iat: u64,
    pub device_id: String,
    pub scopes: Vec<String>,
}

impl GemiAccessClaims {
    pub fn validate(&self, now_unix: u64, required_scope: Option<&str>) -> Result<(), String> {
        if self.iss != GEMI_ACCESS_ISSUER {
            return Err(format!("invalid OIDC issuer `{}` (expected `{GEMI_ACCESS_ISSUER}`)", self.iss));
        }
        if !self.aud.iter().any(|a| a == GEMI_ACCESS_CLIENT_ID || a == PRODUCT_ID) {
            return Err(format!("token audience {:?} does not include `{GEMI_ACCESS_CLIENT_ID}`", self.aud));
        }
        if now_unix >= self.exp {
            return Err(format!("access token expired at {} (now {})", self.exp, now_unix));
        }
        if let Some(req) = required_scope
            && !self.scopes.iter().any(|s| s == req || s == "filmtron:*")
        {
            return Err(format!("access token missing required scope `{req}`"));
        }
        Ok(())
    }
}

/// Signed offline entitlement lease (`gemiguard.offline-lease.v1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineLease {
    pub schema_version: String,
    pub lease_id: String,
    pub product_id: String,
    pub subject_id: String,
    pub device_id: String,
    pub tier: String,
    pub scopes: Vec<String>,
    pub issued_at_unix: u64,
    pub expires_at_unix: u64,
    pub offline_grace_seconds: u64,
    pub preserve_user_project_data: bool,
    pub signature_hex: String,
}

impl OfflineLease {
    /// Canonical byte payload covered by the Ed25519 signature.
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut scopes = self.scopes.clone();
        scopes.sort();
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.schema_version,
            self.lease_id,
            self.product_id,
            self.subject_id,
            self.device_id,
            self.tier,
            scopes.join(","),
            self.issued_at_unix,
            self.expires_at_unix,
            self.offline_grace_seconds,
            self.preserve_user_project_data
        )
        .into_bytes()
    }

    pub fn sign(&mut self, secret_key: &[u8; 32]) {
        let sig = ed25519_sign(secret_key, &self.canonical_payload());
        self.signature_hex = hex_encode(&sig);
    }

    pub fn verify_signature(&self, public_key: &[u8; 32]) -> Result<(), String> {
        if self.schema_version != OFFLINE_LEASE_SCHEMA {
            return Err(format!("unsupported lease schema `{}`", self.schema_version));
        }
        if self.product_id != PRODUCT_ID {
            return Err(format!("lease product `{}` does not match `{PRODUCT_ID}`", self.product_id));
        }
        let sig_bytes = hex_decode_64(&self.signature_hex)?;
        if !ed25519_verify(public_key, &self.canonical_payload(), &sig_bytes) {
            return Err("Ed25519 signature verification failed for offline lease".into());
        }
        Ok(())
    }
}

/// Signed device and lease revocation list (`gemiguard.revocation-list.v1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevocationList {
    pub schema_version: String,
    pub sequence_number: u64,
    pub issued_at_unix: u64,
    pub revoked_lease_ids: Vec<String>,
    pub revoked_device_ids: Vec<String>,
    pub signature_hex: String,
}

impl RevocationList {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut leases = self.revoked_lease_ids.clone();
        leases.sort();
        let mut devices = self.revoked_device_ids.clone();
        devices.sort();
        format!(
            "{}|{}|{}|{}|{}",
            self.schema_version,
            self.sequence_number,
            self.issued_at_unix,
            leases.join(","),
            devices.join(",")
        )
        .into_bytes()
    }

    pub fn sign(&mut self, secret_key: &[u8; 32]) {
        let sig = ed25519_sign(secret_key, &self.canonical_payload());
        self.signature_hex = hex_encode(&sig);
    }

    pub fn verify_signature(&self, public_key: &[u8; 32]) -> Result<(), String> {
        if self.schema_version != REVOCATION_LIST_SCHEMA {
            return Err(format!("unsupported revocation schema `{}`", self.schema_version));
        }
        let sig_bytes = hex_decode_64(&self.signature_hex)?;
        if !ed25519_verify(public_key, &self.canonical_payload(), &sig_bytes) {
            return Err("Ed25519 signature verification failed for revocation list".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntitlementStatus {
    Active,
    OfflineGrace,
    ExpiredLease,
    RevokedDevice,
    RevokedLease,
    InvalidSignature,
}

/// Authoritative entitlement evaluation result.
///
/// Note that `local_editing_allowed` and `preserve_user_project_data` are ALWAYS `true` so
/// offline/expired/revoked states never corrupt or lock out local `.fcproj` projects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitlementDecision {
    pub status: EntitlementStatus,
    pub cloud_mcp_allowed: bool,
    pub local_editing_allowed: bool,
    pub preserve_user_project_data: bool,
    pub reason: String,
}

impl EntitlementDecision {
    fn allow(status: EntitlementStatus, reason: impl Into<String>) -> Self {
        Self {
            status,
            cloud_mcp_allowed: true,
            local_editing_allowed: true,
            preserve_user_project_data: true,
            reason: reason.into(),
        }
    }

    fn deny_cloud(status: EntitlementStatus, reason: impl Into<String>) -> Self {
        Self {
            status,
            cloud_mcp_allowed: false,
            local_editing_allowed: true,
            preserve_user_project_data: true,
            reason: reason.into(),
        }
    }
}

/// Evaluate an `OfflineLease` against the GemiGuard Ed25519 public key, current timestamp,
/// device ID, and optional signed `RevocationList`.
pub fn evaluate_entitlement(
    lease: &OfflineLease,
    public_key: &[u8; 32],
    device_id: &str,
    now_unix: u64,
    revocations: Option<&RevocationList>,
) -> EntitlementDecision {
    if let Err(e) = lease.verify_signature(public_key) {
        return EntitlementDecision::deny_cloud(EntitlementStatus::InvalidSignature, e);
    }
    if lease.device_id != device_id {
        return EntitlementDecision::deny_cloud(
            EntitlementStatus::RevokedDevice,
            format!("lease bound to device `{}` (current device `{device_id}`)", lease.device_id),
        );
    }
    if let Some(rev) = revocations {
        if let Err(e) = rev.verify_signature(public_key) {
            return EntitlementDecision::deny_cloud(EntitlementStatus::InvalidSignature, e);
        }
        if rev.revoked_device_ids.iter().any(|d| d == device_id) {
            return EntitlementDecision::deny_cloud(
                EntitlementStatus::RevokedDevice,
                format!("device `{device_id}` is listed in GemiGuard revocation list #{}", rev.sequence_number),
            );
        }
        if rev.revoked_lease_ids.iter().any(|l| l == &lease.lease_id) {
            return EntitlementDecision::deny_cloud(
                EntitlementStatus::RevokedLease,
                format!("lease `{}` is listed in GemiGuard revocation list #{}", lease.lease_id, rev.sequence_number),
            );
        }
    }
    if now_unix <= lease.expires_at_unix {
        return EntitlementDecision::allow(EntitlementStatus::Active, "offline lease verified and active");
    }
    let grace_deadline = lease.expires_at_unix.saturating_add(lease.offline_grace_seconds);
    if now_unix <= grace_deadline {
        return EntitlementDecision::allow(
            EntitlementStatus::OfflineGrace,
            format!("within offline grace window until {grace_deadline}"),
        );
    }
    EntitlementDecision::deny_cloud(
        EntitlementStatus::ExpiredLease,
        format!("offline lease expired at {} (grace ended at {grace_deadline}); local .fcproj editing remains active", lease.expires_at_unix),
    )
}

fn hex_decode_64(hex: &str) -> Result<[u8; 64], String> {
    if hex.len() != 128 {
        return Err(format!("expected 128-hex-char signature, got {} chars", hex.len()));
    }
    let mut out = [0u8; 64];
    for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0]).ok_or_else(|| "invalid hex character in signature".to_string())?;
        let lo = hex_nibble(chunk[1]).ok_or_else(|| "invalid hex character in signature".to_string())?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ============================================================================
// Pure-Rust SHA-512 (FIPS 180-4) & Ed25519 (RFC 8032)
// ============================================================================

const K512: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
    0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
    0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
    0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

/// Pure-Rust SHA-512 (FIPS 180-4).
pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut h: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];
    let bit_len = (data.len() as u128) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 128) != 112 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(128) {
        let mut w = [0u64; 80];
        for (i, word) in chunk.chunks_exact(8).enumerate() {
            let mut b = [0u8; 8];
            b.copy_from_slice(word);
            w[i] = u64::from_be_bytes(b);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K512[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
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

    let mut out = [0u8; 64];
    for (i, word) in h.iter().enumerate() {
        out[i * 8..(i + 1) * 8].copy_from_slice(&word.to_be_bytes());
    }
    out
}

type Fe = [i64; 16];

const GF0: Fe = [0; 16];
const GF1: Fe = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const D: Fe = [
    0x78a3, 0x1359, 0x4dca, 0x75eb, 0xd8ab, 0x4141, 0x0a4d, 0x0070,
    0xe898, 0x7779, 0x4079, 0x8cc7, 0xfe73, 0x2b6f, 0x6cee, 0x5203,
];
const D2: Fe = [
    0xf159, 0x26b2, 0x9b94, 0xebd6, 0xb156, 0x8283, 0x149a, 0x00e0,
    0xd130, 0xeef3, 0x80f2, 0x198e, 0xfce7, 0x56df, 0xd9dc, 0x2406,
];
const X: Fe = [
    0xd51a, 0x8f25, 0x2d60, 0xc956, 0xa7b2, 0x9525, 0xc760, 0x692c,
    0xdc5c, 0xfdd6, 0xe231, 0xc0a4, 0x53fe, 0xcd6e, 0x36d3, 0x2169,
];
const Y: Fe = [
    0x6658, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666,
    0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666,
];
const I_SQRT: Fe = [
    0xa0b0, 0x4a0e, 0x1b27, 0xc4ee, 0xe478, 0xad2f, 0x1806, 0x2f43,
    0xd7a7, 0x3dfb, 0x0099, 0x2b4d, 0xdf0b, 0x4fc1, 0x2480, 0x2b83,
];

fn car25519(o: &mut Fe) {
    for i in 0..16 {
        o[i] += 1 << 16;
        let c = o[i] >> 16;
        o[(i + 1) * ((i < 15) as usize)] += c - 1 + 37 * (c - 1) * ((i == 15) as i64);
        o[i] -= c << 16;
    }
}

fn sel25519(p: &mut Fe, q: &mut Fe, b: i64) {
    let c = !(b - 1);
    for i in 0..16 {
        let t = c & (p[i] ^ q[i]);
        p[i] ^= t;
        q[i] ^= t;
    }
}

fn pack25519(o: &mut [u8; 32], n: &Fe) {
    let mut t = *n;
    car25519(&mut t);
    car25519(&mut t);
    car25519(&mut t);
    let mut m = GF0;
    for _ in 0..2 {
        m[0] = t[0] - 0xffed;
        for i in 1..15 {
            m[i] = t[i] - 0xffff - ((m[i - 1] >> 16) & 1);
            m[i - 1] &= 0xffff;
        }
        m[15] = t[15] - 0x7fff - ((m[14] >> 16) & 1);
        let b = (m[15] >> 16) & 1;
        m[14] &= 0xffff;
        sel25519(&mut t, &mut m, 1 - b);
    }
    for i in 0..16 {
        o[2 * i] = (t[i] & 0xff) as u8;
        o[2 * i + 1] = (t[i] >> 8) as u8;
    }
}

fn neq25519(a: &Fe, b: &Fe) -> bool {
    let mut c = [0u8; 32];
    let mut d = [0u8; 32];
    pack25519(&mut c, a);
    pack25519(&mut d, b);
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= c[i] ^ d[i];
    }
    diff != 0
}

fn par25519(a: &Fe) -> u8 {
    let mut d = [0u8; 32];
    pack25519(&mut d, a);
    d[0] & 1
}

fn unpack25519(n: &[u8; 32]) -> Fe {
    let mut o = GF0;
    for i in 0..16 {
        o[i] = (n[2 * i] as i64) + ((n[2 * i + 1] as i64) << 8);
    }
    o[15] &= 0x7fff;
    o
}

fn fe_add(a: &Fe, b: &Fe) -> Fe {
    let mut o = GF0;
    for i in 0..16 {
        o[i] = a[i] + b[i];
    }
    o
}

fn fe_sub(a: &Fe, b: &Fe) -> Fe {
    let mut o = GF0;
    for i in 0..16 {
        o[i] = a[i] - b[i];
    }
    o
}

fn fe_mul(a: &Fe, b: &Fe) -> Fe {
    let mut t = [0i64; 31];
    for i in 0..16 {
        for j in 0..16 {
            t[i + j] += a[i] * b[j];
        }
    }
    for i in 0..15 {
        t[i] += 38 * t[i + 16];
    }
    let mut o = GF0;
    o.copy_from_slice(&t[..16]);
    car25519(&mut o);
    car25519(&mut o);
    o
}

fn fe_sq(a: &Fe) -> Fe {
    fe_mul(a, a)
}

fn inv25519(i: &Fe) -> Fe {
    let mut c = *i;
    for a in (0..=253).rev() {
        c = fe_sq(&c);
        if a != 2 && a != 4 {
            c = fe_mul(&c, i);
        }
    }
    c
}

fn pow2523(i: &Fe) -> Fe {
    let mut c = *i;
    for a in (0..=250).rev() {
        c = fe_sq(&c);
        if a != 1 {
            c = fe_mul(&c, i);
        }
    }
    c
}

fn add_point(p: &mut [Fe; 4], q: &[Fe; 4]) {
    let a = fe_sub(&p[1], &p[0]);
    let t = fe_sub(&q[1], &q[0]);
    let a = fe_mul(&a, &t);
    let b = fe_add(&p[0], &p[1]);
    let t = fe_add(&q[0], &q[1]);
    let b = fe_mul(&b, &t);
    let c = fe_mul(&p[3], &q[3]);
    let c = fe_mul(&c, &D2);
    let d = fe_mul(&p[2], &q[2]);
    let d = fe_add(&d, &d);
    let e = fe_sub(&b, &a);
    let f = fe_sub(&d, &c);
    let g = fe_add(&d, &c);
    let h = fe_add(&b, &a);
    p[0] = fe_mul(&e, &f);
    p[1] = fe_mul(&h, &g);
    p[2] = fe_mul(&g, &f);
    p[3] = fe_mul(&e, &h);
}

fn cswap(p: &mut [Fe; 4], q: &mut [Fe; 4], b: u8) {
    for i in 0..4 {
        sel25519(&mut p[i], &mut q[i], b as i64);
    }
}

fn pack_point(r: &mut [u8; 32], p: &[Fe; 4]) {
    let zi = inv25519(&p[2]);
    let tx = fe_mul(&p[0], &zi);
    let ty = fe_mul(&p[1], &zi);
    pack25519(r, &ty);
    r[31] ^= par25519(&tx) << 7;
}

fn scalarmult(q: &mut [Fe; 4], p: &[Fe; 4], s: &[u8; 32]) {
    q[0] = GF0;
    q[1] = GF1;
    q[2] = GF1;
    q[3] = GF0;
    let mut x = *p;
    for i in (0..=255).rev() {
        let b = (s[i >> 3] >> (i & 7)) & 1;
        cswap(q, &mut x, b);
        add_point(&mut x, q);
        let q_copy = *q;
        add_point(q, &q_copy);
        cswap(q, &mut x, b);
    }
}

fn scalarbase(p: &mut [Fe; 4], s: &[u8; 32]) {
    let q = [X, Y, GF1, fe_mul(&X, &Y)];
    scalarmult(p, &q, s);
}

fn unpackneg(r: &mut [Fe; 4], p: &[u8; 32]) -> bool {
    r[1] = unpack25519(p);
    r[2] = GF1;
    let t = fe_sq(&r[1]);
    let mut chk = fe_mul(&t, &D);
    let num = fe_sub(&t, &r[2]);
    let den = fe_add(&chk, &r[2]);
    let den2 = fe_sq(&den);
    let den4 = fe_sq(&den2);
    let den6 = fe_mul(&den2, &den4);
    let num_den3 = fe_mul(&num, &fe_mul(&den2, &den));
    let num_den7 = fe_mul(&num, &fe_mul(&den6, &den));
    let mut x = fe_mul(&num_den3, &pow2523(&num_den7));
    chk = fe_mul(&den, &fe_sq(&x));
    if neq25519(&chk, &num) {
        x = fe_mul(&x, &I_SQRT);
    }
    chk = fe_mul(&den, &fe_sq(&x));
    if neq25519(&chk, &num) {
        return false;
    }
    if par25519(&x) == (p[31] >> 7) {
        x = fe_sub(&GF0, &x);
    }
    r[0] = x;
    r[3] = fe_mul(&r[0], &r[1]);
    true
}

const L: [i64; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

fn mod_l(r: &mut [u8; 32], x: &mut [i64; 64]) {
    for i in (32..=63).rev() {
        let mut carry = 0i64;
        for j in (i - 32)..(i - 12) {
            x[j] += carry - 16 * x[i] * L[j - (i - 32)];
            carry = (x[j] + 128) >> 8;
            x[j] -= carry << 8;
        }
        x[i - 12] += carry;
        x[i] = 0;
    }
    let mut carry = 0i64;
    for j in 0..32 {
        x[j] += carry - (x[31] >> 4) * L[j];
        carry = x[j] >> 8;
        x[j] &= 255;
    }
    for j in 0..32 {
        x[j] -= carry * L[j];
    }
    for i in 0..32 {
        x[i + 1] += x[i] >> 8;
        r[i] = (x[i] & 255) as u8;
    }
}

fn reduce(r: &mut [u8; 32], h: &[u8; 64]) {
    let mut x = [0i64; 64];
    for i in 0..64 {
        x[i] = h[i] as i64;
    }
    mod_l(r, &mut x);
}

/// Derive the 32-byte RFC 8032 Ed25519 public key from a 32-byte secret seed.
pub fn ed25519_public_key(secret_seed: &[u8; 32]) -> [u8; 32] {
    let mut d = sha512(secret_seed);
    d[0] &= 248;
    d[31] &= 127;
    d[31] |= 64;
    let mut s = [0u8; 32];
    s.copy_from_slice(&d[..32]);
    let mut p = [GF0; 4];
    scalarbase(&mut p, &s);
    let mut pk = [0u8; 32];
    pack_point(&mut pk, &p);
    pk
}

/// Sign `message` with a 32-byte RFC 8032 Ed25519 secret seed, returning a 64-byte signature.
pub fn ed25519_sign(secret_seed: &[u8; 32], message: &[u8]) -> [u8; 64] {
    let mut d = sha512(secret_seed);
    d[0] &= 248;
    d[31] &= 127;
    d[31] |= 64;
    let mut a = [0u8; 32];
    a.copy_from_slice(&d[..32]);
    let pk = ed25519_public_key(secret_seed);

    let mut prefix_msg = Vec::with_capacity(32 + message.len());
    prefix_msg.extend_from_slice(&d[32..64]);
    prefix_msg.extend_from_slice(message);
    let r_hash = sha512(&prefix_msg);
    let mut r_scalar = [0u8; 32];
    reduce(&mut r_scalar, &r_hash);

    let mut p = [GF0; 4];
    scalarbase(&mut p, &r_scalar);
    let mut r_bytes = [0u8; 32];
    pack_point(&mut r_bytes, &p);

    let mut hram_input = Vec::with_capacity(64 + message.len());
    hram_input.extend_from_slice(&r_bytes);
    hram_input.extend_from_slice(&pk);
    hram_input.extend_from_slice(message);
    let hram = sha512(&hram_input);
    let mut k_scalar = [0u8; 32];
    reduce(&mut k_scalar, &hram);

    let mut x = [0i64; 64];
    for i in 0..32 {
        x[i] = r_scalar[i] as i64;
    }
    for i in 0..32 {
        for j in 0..32 {
            x[i + j] += (k_scalar[i] as i64) * (a[j] as i64);
        }
    }
    let mut s_bytes = [0u8; 32];
    mod_l(&mut s_bytes, &mut x);

    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_bytes);
    sig[32..64].copy_from_slice(&s_bytes);
    sig
}

/// Verify a 64-byte RFC 8032 Ed25519 signature against `public_key` and `message`.
pub fn ed25519_verify(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let mut q = [GF0; 4];
    if !unpackneg(&mut q, public_key) {
        return false;
    }
    let mut r_bytes = [0u8; 32];
    r_bytes.copy_from_slice(&signature[..32]);
    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&signature[32..64]);
    if (s_bytes[31] & 224) != 0 {
        return false;
    }

    let mut hram_input = Vec::with_capacity(64 + message.len());
    hram_input.extend_from_slice(&r_bytes);
    hram_input.extend_from_slice(public_key);
    hram_input.extend_from_slice(message);
    let hram = sha512(&hram_input);
    let mut k_scalar = [0u8; 32];
    reduce(&mut k_scalar, &hram);

    let mut p = [GF0; 4];
    scalarmult(&mut p, &q, &k_scalar);
    let mut sb = [GF0; 4];
    scalarbase(&mut sb, &s_bytes);
    add_point(&mut p, &sb);
    let mut check = [0u8; 32];
    pack_point(&mut check, &p);

    let mut diff = 0u8;
    for i in 0..32 {
        diff |= r_bytes[i] ^ check[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_challenge_and_oidc_contract_validation() {
        // RFC 7636 Appendix B test vector
        let rfc_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let rfc_challenge = PkceChallenge::compute_s256_challenge(rfc_verifier);
        assert_eq!(rfc_challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");

        let pkce = PkceChallenge::generate().expect("generate PKCE");
        assert!(pkce.verify(&pkce.code_verifier));
        assert!(!pkce.verify("wrong-verifier"));

        let req = OidcAuthorizationRequest::new(&pkce, None);
        let url = req.authorization_url();
        assert!(url.starts_with("https://auth.gemitron.net/oauth2/authorize?"));
        assert!(url.contains("client_id=filmtron-desktop"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("filmtron:edit"));

        let claims = GemiAccessClaims {
            iss: GEMI_ACCESS_ISSUER.into(),
            sub: "user-42".into(),
            aud: vec![GEMI_ACCESS_CLIENT_ID.into()],
            exp: 2_000_000_000,
            iat: 1_900_000_000,
            device_id: "win-ws-01".into(),
            scopes: vec!["filmtron:read".into(), "filmtron:edit".into()],
        };
        assert!(claims.validate(1_950_000_000, Some("filmtron:edit")).is_ok());
        assert!(claims.validate(2_000_000_001, Some("filmtron:edit")).is_err());
        assert!(claims.validate(1_950_000_000, Some("filmtron:render")).is_err());
    }

    #[test]
    fn rfc8032_ed25519_test_vector_1_and_offline_lease_lifecycle() {
        // RFC 8032 Section 7.1 TEST 1 (empty message)
        let seed: [u8; 32] = [
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
            0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
        ];
        let pk = ed25519_public_key(&seed);
        assert_eq!(
            hex_encode(&pk),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let sig = ed25519_sign(&seed, b"");
        assert_eq!(
            hex_encode(&sig),
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
        );
        assert!(ed25519_verify(&pk, b"", &sig));
        assert!(!ed25519_verify(&pk, b"tampered", &sig));

        // Signed GemiGuard OfflineLease lifecycle
        let mut lease = OfflineLease {
            schema_version: OFFLINE_LEASE_SCHEMA.into(),
            lease_id: "lease-filmtron-001".into(),
            product_id: PRODUCT_ID.into(),
            subject_id: "user-42".into(),
            device_id: "win-workstation-01".into(),
            tier: "pro".into(),
            scopes: vec!["filmtron:read".into(), "filmtron:edit".into(), "filmtron:render".into()],
            issued_at_unix: 1_700_000_000,
            expires_at_unix: 1_700_086_400,
            offline_grace_seconds: 3_600,
            preserve_user_project_data: true,
            signature_hex: String::new(),
        };
        lease.sign(&seed);

        // 1. Active lease
        let active = evaluate_entitlement(&lease, &pk, "win-workstation-01", 1_700_010_000, None);
        assert_eq!(active.status, EntitlementStatus::Active);
        assert!(active.cloud_mcp_allowed);
        assert!(active.local_editing_allowed);
        assert!(active.preserve_user_project_data);

        // 2. Offline grace window
        let grace = evaluate_entitlement(&lease, &pk, "win-workstation-01", 1_700_087_000, None);
        assert_eq!(grace.status, EntitlementStatus::OfflineGrace);
        assert!(grace.cloud_mcp_allowed);
        assert!(grace.local_editing_allowed);

        // 3. Expired past grace -> cloud disabled, local .fcproj editing still allowed!
        let expired = evaluate_entitlement(&lease, &pk, "win-workstation-01", 1_700_095_000, None);
        assert_eq!(expired.status, EntitlementStatus::ExpiredLease);
        assert!(!expired.cloud_mcp_allowed);
        assert!(expired.local_editing_allowed);
        assert!(expired.preserve_user_project_data);

        // 4. Revoked device -> cloud disabled, local .fcproj editing still allowed!
        let mut rev = RevocationList {
            schema_version: REVOCATION_LIST_SCHEMA.into(),
            sequence_number: 7,
            issued_at_unix: 1_700_020_000,
            revoked_lease_ids: Vec::new(),
            revoked_device_ids: vec!["win-workstation-01".into()],
            signature_hex: String::new(),
        };
        rev.sign(&seed);
        let revoked = evaluate_entitlement(&lease, &pk, "win-workstation-01", 1_700_020_000, Some(&rev));
        assert_eq!(revoked.status, EntitlementStatus::RevokedDevice);
        assert!(!revoked.cloud_mcp_allowed);
        assert!(revoked.local_editing_allowed);
        assert!(revoked.preserve_user_project_data);

        // 5. Tampered lease -> InvalidSignature, cloud disabled, local .fcproj editing still allowed!
        let mut tampered = lease.clone();
        tampered.tier = "enterprise".into();
        let bad = evaluate_entitlement(&tampered, &pk, "win-workstation-01", 1_700_010_000, None);
        assert_eq!(bad.status, EntitlementStatus::InvalidSignature);
        assert!(!bad.cloud_mcp_allowed);
        assert!(bad.local_editing_allowed);
        assert!(bad.preserve_user_project_data);
    }
}
