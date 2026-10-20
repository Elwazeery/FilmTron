# FilmTron — Native Control / Cloud MCP Gap Audit

**Source inspection:** 2026-10-20 · **Status:** SOURCE-INSPECTED, CLOUD EXECUTION NOT ENABLED / NO REAL CLIENT ACCEPTANCE.

## Existing actual upstream native control

FilmCraft control protocol starts at 127.0.0.1 and accepts unauthenticated JSON lines. It has a 4-MiB per-line bound, 16-connection cap and rejects HTTP request lines/invalid JSON, but its own official docs explicitly state 'The port has no authentication'. Loopback is not an authorization boundary among local processes.

Source locations: `apps/filmcraft/src/control_server.rs`, `docs/control-protocol.md`, native command registry/automation sources; all source owned by this product. This control is NOT GemiMCP's cloud server.

## Native commands already advertised

`engine.commands`, `engine.execute`, `ui.inspect`, `ui.elements`, `ui.set`, `ui.menu.list`, `ui.drag`, `ui.click`, `ui.screenshot`, `ui.timeline.locate`, `ui.timeline.hit`, `perf.stats`, export/job commands. Exact UI parity still unverified.

Do not replace engine commands with synthetic mouse clicks. Keep UI manipulation for dialog/window operations and verified source state. Inventory actual capabilities/parameters and dynamic properties, typed input/output schemas, stable IDs, safe apply/verify/undo and real screenshot/export evidence to satisfy the future **100% MCP parity acceptance goal**, measured against implemented GUI actions.

## Mandatory next remediation

P0 BLOCKER BEFORE ANY CLOUD DEVICE BRIDGE: implement native per-launch secret / protected IPC, authenticate the first request before any method dispatch, deny empty/stale/wrong credentials, bound auth deadline, token storage in OS-protected location and per-project asset roots. Preserve legacy CLI with explicit safe migration/deprecation and independent tests. No unauthenticated request may reach ControlRequest. Reconnect/timeout and reply status must be reconciled.

Required secure path: `ChatGPT → GemiTron88/GemiMCP cloud MCP (OIDC + Gemi Access + GemiGuard where relevant) → scoped outbound Desktop Agent → authenticated local command/IPC → native editor readback/receipt`. Never open the local port to a public interface. The cloud MCP implementation is maintained centrally in `GemiTron88/GemiMCP`, native editor/API corrections only here.

**Acceptance evidence needed:** independent wrong-token/bypass/rogue-process test, UI↔native command parity manifest and tests, physical Windows/macOS native save/undo/readback, device disconnect/recovery, authoritative job receipt. Do not declare Cloud MCP 'done' from the presence of the local control method names.

## Source-only changes and verified limits

- Source fork ancestry verified via GitHub compare against pinned original Git SHA, original license files preserved.
- First visible Tron title/Windows metadata/CLI rebrand committed.
- No native cloud bridge or real remote edit receipt has been implemented.
- Existing executables/source crate names/APP_ID and original project file extensions retained until safe migration, icons and full visual redesign remain open.
