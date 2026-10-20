# Product charter — fork, native editing, cloud MCP, SSO and licensing

**Owner-approved 2026-10-20. Documentation-only.** See centralized runtime owner [GemiMCP](https://github.com/GemiTron88/GemiMCP) and its [architecture](https://github.com/GemiTron88/GemiMCP/blob/main/docs/architecture/01-CLOUD-MCP-PLATFORM.md).

## Common binding product/architecture rules

1. One independent desktop editor, its own project schema, undo stack, asset storage, renderer and native command engine. **Never merge into or depend on GemiMotion**, and never substitute a web mockup for a professional workstation binary. First-class Windows/macOS, measured support of any extra platforms.
2. **Real fork:** inspect/pin upstream version + current license; import upstream Git history and establish upstream remote for periodic audited sync. Preserve LICENSE/NOTICE, dependencies' terms and author credits; rebrand original marks only where rights permit; record added code separately. Avoid force push destroying documentation/history.
3. **Full native/MCP parity:** inventory ALL implemented menus/toolbar/keyboard shortcuts/inspector and nested object properties, session/UI state, dialogs, batch ops, exporter features. Every meaningful UI operation maps to typed native command/query with stable object IDs, deterministic readback, expected revision, scope, undo/redo or explicit non-reversible status and real preview. Runtime MCP transport/server implementations **only in GemiMCP repo**, not an extra competing gateway in this repo. This repo owns native command adapters/manifest/tests.
4. **Cloud execution:** protected `mcp.gemitron.net` routes in central repo, direct standalone product MCP AND federated GemiMCP availability. Outbound-only device agent on Windows/macOS, no public open ports, no shell/SQL, allowlisted media roots. Offline native edit remains available subject to GemiGuard local lease; ChatGPT remote control requires a connected device.
5. **Unified account/license:** only OIDC `auth.gemitron.net` + Studio-embedded Gemi Access grants. Only GemiGuard product entitlement/signed offline leases; never write local identity/seat DB or duplicate centralized grant writer. Desktop uses system browser PKCE, secure OS keychain, explicit sign-out/revoke, per-device capability grants. No mandatory live login just to open owned local projects.
6. **Design:** consume pinned generated GemiTokens and GemiFontSans (with tested platform shaping), high-performance original **Inferno Pixel Glass** aesthetic, product-specific accents, dark/light, keyboard/pro dock layout, Arabic/English mixed-direction text, real screenshots and accessibility. Accurate video/image color never tinted by app chrome. Restrict costly blur/animation near preview/canvas.
7. **Quality:** functionally real 2–3 minute editing journey + 100% command parity *target*, actual physical workstation input/preview/save/reopen/undo/export; negative tests for wrong project/device, grant, token, lease, replay, unavailable provider, corrupt source, crash after apply and graceful recovery. Report unsupported separately, never false ACCEPTED status.
8. **AI & cost:** ChatGPT connected via cloud MCP is the directing brain; no local chatbot, no paid LLM/media API bundled, no assertion that a server can perform ongoing agent reasoning unaided; optional neural tools clearly require approved provider availability/cost.
9. **Release:** source-based tasks follow installed ExecutionKit, native OS CI + GPU tests as available, versioned signed release and safe rollback; every completed claim needs commit SHA, tests, screenshot/export and native readback evidence. First verify real upstream feature gaps, then implement only demonstrably missing/high-value features.

## Native MCP baseline command families

Required semantic coverage: `session.*` `project.*` `assets.*` `document.inspect` `object.list/get/create/update/delete` `property.enumerate/get/set` `selection.*` `history.undo/redo` `preview.frame/capture` `render/export.*` `jobs.status/cancel` `ui.layout/panels/selection` and domain-specific namespaces documented below. **These are planned families** — discover real native handler names before implementing; don't register placeholders as active tools.

Required call pattern: inspect → prepare non-mutating diff & expected revision → authorize/confirm if needed → native apply/receipt → preview/readback/verify → optional undo. Long jobs report progress and cancellation with a durable status. Batch operations must be atomic only where native transaction is actually supported; otherwise report partial and compensating edits.

## Upstream/standalone acceptance boundary

Standalone MCP means direct cloud tool access scoped to THIS product even if federated GemiMCP discovery is unavailable, while sharing the same one GemiMCP-hosted protocol/security stack. It does NOT mean standalone account, independent license, new GitHub MCP repo, or new AI model.

## FilmTron-specific scope and missing-feature priority

**Upstream:** `storytold/filmcraft`, a young Rust NLE. Preserve native codec and timeline design where correctly tested; do not blindly rewrite rendering or port to Electron.

**Professional feature coverage to audit against actual implementation:**
- Timeline precision: media bins/assets, source monitor, multi-track linking, markers, snapping, ripple/roll/slip/slide, grouped trim, nest/multicam (qualify what's actually implemented), keyframes and easing.
- Viewer/monitor realtime playback, decoding codecs, proxy/previews, time base drop/non-drop frame, VFR media and A/V synchronization, waveform caching, large project performance and project auto-recovery.
- Effects/transitions, tracking where real, color grading/scopes, color management, text/RTL/diacritics/captions, master/submix/buses, loudness and music/dubbing time alignment.
- Render/export: correct aspect ratios, frame rates, codec matrix, bitrate, alpha/HDR only if verified, source relink and interoperable project formats with loss reports.
- Avoid paid API assumptions: transcript edits and semantic cuts require real available transcription provider; deterministic silence detection is separate and testable offline.

**Additional typed MCP domains:** `film.media.*`, `film.bin.*`, `film.sequence.*`, `film.track.*`, `film.clip.*`, `film.trim.*`, `film.transition.*`, `film.keyframe.*`, `film.grade.*`, `film.audio.*`, `film.caption.*`, `film.proxy.*`, `film.render.*`; use rational frame/timebase values instead of rounding float milliseconds.

**P0 vertical acceptance:** import two video sources and music; create native multitrack sequence, perform exact frame-accurate clip and title edits through authenticated cloud MCP to local running FilmTron, then preview, inspect timeline revision, undo, save/reopen and export physically decodable MP4 with frame count/time and audio QC.
