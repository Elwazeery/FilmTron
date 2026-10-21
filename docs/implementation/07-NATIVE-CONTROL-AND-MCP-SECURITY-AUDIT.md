# FilmTron — Native Control, GemiMCP Parity & Security Audit

**Audit & Remediation Date:** 2026-10-21 · **Status:** REMEDIATED & VERIFIED (`FILM-010`, `FILM-003`, `FILM-005`, `FILM-006`, `FILM-007`).

## 1. P0 Native Control Security Remediation (`FILM-010`)

The upstream FilmCraft local control server (`apps/filmcraft/src/control_server.rs`, `crates/automation/src/server.rs`, `crates/automation/src/bridge.rs`) previously accepted unauthenticated JSON-line commands on `127.0.0.1` and used a 4 MiB line buffer without an initial authentication handshake or workspace path confinement.

FilmTron remediates all P0 control-channel vulnerabilities in [`crates/automation/src/security.rs`](../../crates/automation/src/security.rs):

| Control Control / Defense | Implementation in `filmcraft-automation::security` | Verification |
|---|---|---|
| **CSPRNG Per-Launch Token** | `generate_token()` generates a 256-bit (32-byte / 64-hex) token using OS CSPRNG (`getrandom`). | `security::tests::token_generation_is_64_hex_and_validates` |
| **Mandatory Pre-Dispatch Handshake** | First request on any TCP or stdio session must be `{"id": 0, "method": "auth", "params": {"token": "<64-hex>"}}`. Any other method before authentication returns `{"ok": false, "code": "AUTH_REQUIRED"}` without reaching `ControlRequest` or `Session::execute`. | `control_server::tests::rejects_unauthenticated_request_before_dispatch`, `control_server::tests::rejects_wrong_token_and_allows_valid_token` |
| **Constant-Time Token Comparison** | `token_matches(provided, expected)` compares SHA-256 digests (`sha256`) in constant time. | `security::tests::constant_time_token_comparison_rejects_mismatch` |
| **10-Second Auth Deadline & 30-Second I/O Timeout** | `AUTH_DEADLINE = Duration::from_secs(10)`, `IO_TIMEOUT = Duration::from_secs(30)`, enforced by `AuthDeadlineStream`. | `security::tests::auth_deadline_and_line_bounds_are_enforced` |
| **1 MiB Request Frame Cap & 16-Connection Cap** | `MAX_REQUEST_BYTES = 1_048_576` (1 MiB) via `read_bounded_line` + `discard_rest_of_line`; `MAX_CONNECTIONS = 16` via `ConnectionLimiter`. | `control_server::tests::caps_concurrent_connections` |
| **Loopback-Only Binding** | `validate_loopback_bind_addr` rejects non-loopback IPv4/IPv6 addresses (`0.0.0.0`, `::`, LAN IPs) so the control port is never exposed to public interfaces. | `security::tests::loopback_bind_validation_blocks_public_interfaces` |
| **Windows DACL Hardening (`icacls`)** | `create_token_file` + `restrict_windows_token_acl` + `verify_windows_token_acl` strip inherited ACLs (`icacls /inheritance:r`) and grant access exclusively to the current user (`SID` / `USERNAME`) and `S-1-5-18` (`SYSTEM`), rejecting `WD`, `BU`, `AU`, `BG`, `AN`. | `security::tests::token_file_roundtrip_and_windows_acl_verification` |
| **Authorized Workspace Confinement** | `AuthorizedWorkspace` (`FILMTRON_ALLOWED_ROOTS`) canonicalizes project and media paths, rejecting `..` traversal, NUL bytes, Windows reserved device names (`CON`, `NUL`, `COM1`, `LPT1`), and paths outside authorized roots. | `security::tests::workspace_confines_paths_and_blocks_traversal_and_devices` |

## 2. Optimistic Concurrency (`expectedRevision`) & Idempotency (`FILM-003`)

Every mutating command dispatched through `execute_guarded` (`crates/automation/src/security.rs`) or the UI control handler (`crates/ui-egui/src/control.rs`) supports:
- **`expectedRevision`**: Compares against `Session::revision`. On mismatch, returns `REVISION_CONFLICT` with the current authoritative `revision` without mutating state.
- **`idempotencyKey`**: Tracked in a bounded 256-entry `IdempotencyCache` storing `(command, payload_sha256, result, revision)`. Replaying the same key and payload returns the cached result with `"idempotentReplay": true`; reusing the same key with a different command or payload returns `IDEMPOTENCY_CONFLICT`.

## 3. Cloud GemiMCP Parity & Outbound Desktop Bridge (`FILM-005`)

- **Cloud Owner**: `GemiTron88/GemiMCP` (`wss://mcp.gemitron.net/v1/bridge/filmtron`, `outbound_only: true`, `no_public_local_listeners: true`).
- **Contract Manifest**: [`contracts/gemimcp-product-filmtron.v1.json`](../../contracts/gemimcp-product-filmtron.v1.json), generated and verified by [`crates/automation/src/gemimcp.rs`](../../crates/automation/src/gemimcp.rs).
- **Coverage**:
  - **33 semantic `film.*` tools** across `projects_and_media`, `sequences_and_timebases`, `edits_trims_and_clips`, `effects_and_transitions`, `keyframes_and_properties`, `titles_and_captions`, `color_and_scopes`, `audio_and_dsp`, `render_and_export`, `undo_redo_and_recovery`, and `ui_and_workspace`.
  - **532 native `film.cmd.<id>` tools** mapping 1:1 to every `CommandSpec` in `filmcraft_engine::command_specs()`.
- **Permission Scopes**: `filmtron:read`, `filmtron:edit`, `filmtron:render`, `filmtron:ui`.

## 4. Gemi Access OIDC PKCE & GemiGuard Offline Entitlements (`FILM-006`)

Implemented in [`crates/automation/src/gemiguard.rs`](../../crates/automation/src/gemiguard.rs):
- **OIDC PKCE (`S256`)**: `PkceChallenge` (verified against RFC 7636 Appendix B) and `OidcAuthorizationRequest` targeting `https://auth.gemitron.net/oauth2/authorize` (`client_id = "filmtron-desktop"`).
- **Ed25519 Offline Leases & Revocation**: Pure-Rust FIPS 180-4 SHA-512 + RFC 8032 Ed25519 signature verification (`ed25519_verify`, verified against RFC 8032 Section 7.1 Test Vector 1) for `OfflineLease` (`gemiguard.offline-lease.v1`) and `RevocationList` (`gemiguard.revocation-list.v1`).
- **Offline Native Editing Guarantee**: When a lease expires or a device is revoked, `evaluate_entitlement` disables cloud MCP bridge execution (`cloud_mcp_allowed = false`) while keeping local native `.fcproj` editing, saving, and export 100% active (`local_editing_allowed = true`, `preserve_user_project_data = true`).
