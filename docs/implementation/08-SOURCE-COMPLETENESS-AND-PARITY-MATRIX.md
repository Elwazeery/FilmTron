# 08 — FilmTron Source Completeness, Capability, and Parity Matrix (`FILM-002`)

- **Repository:** `https://github.com/Elwazeery/FilmTron` (`main`)
- **Upstream Baseline:** `storytold/filmcraft` (`cdf43fd366e378da266cb6d3c94549a45244be1f`)
- **Product Identity:** **FilmTron** (`filmtron`, standalone native NLE in the GemiTron Creative Suite)

---

## 1. Workspace Architecture & Crate Inventory (60 Packages across L0–L6)

FilmTron enforces strict downward-only crate layering via `cargo run -p xtask --locked -- layers` and `#[forbid(unsafe_code)]` across every workspace crate except `crates/platform` (which isolates OS FFI for Windows Media Foundation, macOS VideoToolbox, and Linux VA-API/NVDEC).

| Layer | Crates | Responsibility & Verified Status |
|---|---|---|
| **L0 — Foundation** | `filmcraft-time`, `filmcraft-geom`, `filmcraft-color`, `filmcraft-bitstream`, `filmcraft-testkit` | Exact rational `Tick` timebase (`254,016,000,000` ticks/sec), 2D/3D transforms & Bézier paths, Rec.709 / Rec.2020 / Rec.2100 PQ & HLG / S-Log3 / ACEScct color spaces, 1D/3D `.cube` LUTs, bitstream readers/writers. |
| **L0 — Containers** | `filmcraft-isobmff`, `filmcraft-matroska`, `filmcraft-avi`, `filmcraft-mxf`, `filmcraft-mpegts`, `filmcraft-ogg`, `filmcraft-riff`, `filmcraft-cfb` | Pure-Rust demuxers and muxers for MP4/MOV (`isobmff`), MKV/WebM (`matroska`), AVI, SMPTE MXF, MPEG-TS, Ogg, WAV/AIFF (`riff`), and compound files (`cfb`). |
| **L0 — Video Codecs** | `filmcraft-h264`, `filmcraft-h264enc`, `filmcraft-hevc`, `filmcraft-vp8`, `filmcraft-vp9`, `filmcraft-av1`, `filmcraft-prores`, `filmcraft-dnx`, `filmcraft-mjpeg`, `filmcraft-mpeg2v`, `filmcraft-apv`, `filmcraft-dng` | Pure-Rust decoders for H.264/AVC, HEVC/H.265, VP8, VP9, AV1, Apple ProRes, Avid DNxHD/DNxHR, MJPEG, MPEG-2 Video, APV, and CinemaDNG; pure-Rust H.264 Baseline/Main/High encoder (`h264enc`) + ProRes/DNxHR/MJPEG encoders. |
| **L0 — Audio Codecs** | `filmcraft-aac`, `filmcraft-flac`, `filmcraft-opus`, `filmcraft-mpa`, `filmcraft-ac3` | Pure-Rust AAC-LC decoder & encoder, FLAC decoder & encoder, Opus decoder & encoder, MP1/MP2/MP3 (`mpa`) decoder, and Dolby AC-3 decoder. |
| **L1 — Core Data & Text** | `filmcraft-frame`, `filmcraft-media`, `filmcraft-project`, `filmcraft-audio-dsp`, `filmcraft-text` | Pixel buffers (RGBA8, RGBA16F, YUV420/422/444 8/10-bit), procedural generators (SMPTE bars, tone, timecode, color matte), project/bin/sequence/track/clip/marker/transition model, biquad EQ / compressor / limiter / LUFS (`ITU-R BS.1770-4`) DSP, OpenType font shaping + Arabic/Hebrew Unicode BiDi (`rtl.rs`). |
| **L2 — Domain Engines** | `filmcraft-edit`, `filmcraft-codecs`, `filmcraft-keyframe`, `filmcraft-effects`, `filmcraft-audio`, `filmcraft-captions`, `filmcraft-speech`, `filmcraft-tts`, `filmcraft-tts-text`, `filmcraft-interchange` | Frame-accurate ripple/roll/slip/slide/insert/overwrite/razor/multicam/nest editing, unified codec registry & hardware decode bridge, Bézier/Hold/Linear keyframe evaluation, 50+ video/audio effects & Lumetri Color grading, multitrack audio mixer & ducking, SRT/VTT/CEA-608/708 captions, offline Whisper speech-to-text, FCP7 XML / EDL / AAF / OTIO / Premiere `.prproj` interchange. |
| **L3 — Render, GPU & I/O** | `filmcraft-render`, `filmcraft-gpu`, `filmcraft-scopes`, `filmcraft-playback`, `filmcraft-export`, `filmcraft-format`, `filmcraft-golden` | CPU & `wgpu` shader compositor, real-time frame prefetch & A/V clock sync, Vectorscope/Waveform/Parade/Histogram scopes, MP4/MOV/MKV/WAV/FLAC/AAC/Image-Sequence export pipeline & render queue, `.fcproj` atomic persistence & autosave recovery, golden visual regression harness. |
| **L4 — Command Engine** | `filmcraft-engine` | `Session` state machine, undo/redo history, crash-recovery journal, `Preferences` schema (`PREFS_VERSION = 2`), and 270+ registered commands in `command_specs()`. |
| **L5 — Platform, UI & Automation** | `filmcraft-platform`, `filmcraft-ui-egui`, `filmcraft-automation` | OS hardware codecs & native menus, full docking `egui` NLE interface (Project Bin, Source/Program Monitors, Timeline, Effect Controls, Lumetri Color, Scopes, Audio Track/Clip Mixers, Essential Graphics, Essential Sound, Text/Transcript, Export, Preferences), MCP server, localhost control client/server, GemiMCP product adapter, and GemiGuard verifier. |
| **L6 — Applications** | `filmcraft` (`apps/filmcraft`), `filmcraft-cli` (`apps/filmcraft-cli`), `filmcraft-web` (`apps/filmcraft-web`) | Native desktop application (`wgpu` + `eframe` + `cpal`), headless CLI & MCP/control runner (`filmcraft-cli`), and WebAssembly build target. |

---

## 2. Subsystem Capability Audit

### 2.1 Rational Timebase & Timeline Editing (`filmcraft-time`, `filmcraft-project`, `filmcraft-edit`)
- **Tick Resolution:** `1 second = 254,016,000,000 ticks` ($2^{10} \times 3^4 \times 5^3 \times 7^2 \times 11 \times 13 \times 349$ LCM factor), representing 23.976 (`24000/1001`), 24, 25, 29.97 (`30000/1001`), 30, 50, 59.94 (`60000/1001`), and 60 fps video frames and 44.1 kHz / 48 kHz / 96 kHz audio samples with zero floating-point drift.
- **Editorial Operations:** Insert, overwrite, lift, extract, razor split (`sequence.razor`, `sequence.razorAll`), trim in/out, ripple trim, roll trim, slip, slide, rate stretch, speed/duration (`clip.speed`, reverse, ripple, optical flow / frame blend time interpolation), time remapping keyframes, freeze frame / frame hold, link/unlink A/V, group/ungroup, nest (`sequence.nest`), multicam source sequence creation & live angle cutting (`sequence.newMulticam`, `sequence.cutToCamera`), markers, and snapping.

### 2.2 Video Effects, Essential Graphics, Titles, Captions & Color (`filmcraft-effects`, `filmcraft-text`, `filmcraft-scopes`)
- **Intrinsic & Stackable Effects:** Motion (Position, Scale, Uniform Scale, Rotation, Anchor Point, Anti-flicker), Opacity & 27 blend modes, Crop, Time Remapping, Corner Pin, Transform, Gaussian/Directional/Camera Blur, Sharpen, Unsharp Mask, Chroma Key / Ultra Key, Luma Key, Track Matte Key, Mosaic, Posterize, Vignette, Glow, Drop Shadow, Stroke, Warp Stabilizer, Lens Distortion, VR Projection.
- **Masking & Tracking:** Rectangle, Ellipse, and Pen Bézier masks on any effect (including Opacity and Lumetri Color) with feather, opacity, expansion, inversion, and forward/backward planar mask tracking (`mask.track`).
- **Lumetri Color & Scopes:** Basic Correction (Input LUT, Temperature, Tint, Exposure, Contrast, Highlights, Shadows, Whites, Blacks, Saturation, Vibrance), Creative (Look LUT, Faded Film, Sharpen, Vibrance, Saturation, Shadow/Highlight Tint, Tint Balance), Curves (RGB & Hue vs Sat/Hue/Luma), Color Wheels & Match, HSL Secondary, Vignette, and HDR PQ/HLG tone mapping. Scopes include Vectorscope (YUV/HLS), Waveform (RGB/Luma/YC), Parade (RGB/YUV/RGB-White), and Histogram.
- **Titles, Graphics & RTL:** Multi-layer text and vector shapes (`graphics.newText`, `graphics.newRect`, `graphics.newEllipse`), built-in lower-third and title templates, roll/crawl credits, and Unicode BiDi (`crates/ui-egui/src/rtl.rs`, `crates/text`) for Arabic and Hebrew right-to-left scripts.
- **Captions & Transcription:** Closed/open caption tracks (`caption.addTrack`, `caption.add`, `caption.edit`, `caption.split`, `caption.merge`, `caption.importSrt`, `caption.exportSrt`, `caption.burnIn`), plus offline Whisper transcription (`transcript.transcribe`, `transcript.createCaptions`).

### 2.3 Audio Engine, Mixing & Loudness (`filmcraft-audio-dsp`, `filmcraft-audio`, `filmcraft-playback`)
- **Clip & Track DSP:** Clip gain (dB), normalize peak (`audio.normalizeMaxPeak`), normalize loudness (`ITU-R BS.1770-4` LUFS), stereo pan, channel mapping (Mono, Stereo, 5.1, Adaptive), constant-power / exponential audio crossfades, Parametric EQ, Compressor, Limiter, DeEsser, Reverb, Delay, High-Pass / Low-Pass filters, and Essential Sound auto-ducking (`essentialSound.generateDucking`).
- **Track & Submix Routing:** Per-track volume, pan, mute, solo, voice-over record routing, track keyframe automation modes (`Off`, `Read`, `Latch`, `Touch`, `Write`), submix tracks, and master loudness radar/meter.

### 2.4 Export & Render Pipeline (`filmcraft-export`)
- **Containers & Codecs:**
  - `.mp4` / `.mov` (`H.264` + `AAC` or `PCM`, `ProRes`, `DNxHR`)
  - `.mkv` (`H.264` + `Opus` / `FLAC` / `AAC`)
  - `.avi` (`MJPEG` + `PCM`)
  - Audio-only `.wav`, `.aiff`, `.flac`, `.aac`, `.m4a`, `.ogg` (`Opus`)
  - Still frames & numbered image sequences (`.png`, `.jpg`, `.bmp`, `.tga`, `.ppm`)
- **Export Verification:** Pure-Rust container & codec round-trip decoding verifies exact frame count, dimensions, frame rate, non-black visual signal, and decoded audio sample duration.

---

## 3. Measured Defects & Gaps Remediated in FilmTron

| ID | Severity | Subsystem | Measured Defect / Gap | Remediation Implemented |
|---|---|---|---|---|
| **FILM-010** | **P0 Blocker** | `apps/filmcraft/src/control_server.rs`, `crates/automation` | Localhost TCP JSON-lines control server accepted unauthenticated connections and immediately dispatched arbitrary commands (`project.open`, `export.now`, `engine.execute`) without token verification, connection limits, or workspace path confinement. | Implemented `crates/automation/src/security.rs` with 256-bit CSPRNG token generation, constant-time `token_matches`, mandatory first-frame `{"method":"auth"}` handshake with 10s deadline, `ConnectionLimiter` (max 8 concurrent connections), owner-only token file with Windows `icacls` SDDL verification, and `AuthorizedWorkspace` canonical root confinement. |
| **FILM-003** | **High** | `crates/automation/src/server.rs`, `crates/ui-egui/src/control.rs` | Mutating commands lacked optimistic concurrency revision checks (`expectedRevision`), idempotency replay deduplication (`idempotencyKey`), and authoritative mutation revision receipts. | Added `expectedRevision` conflict rejection (`REVISION_CONFLICT`), a bounded 256-entry `idempotencyKey` replay cache, and authoritative receipts (`revision`, `activeSequenceId`, `projectName`, `dirty`) on all mutations. |
| **FILM-004** | **High** | `crates/ui-egui/src/theme.rs`, `crates/engine/src/settings.rs`, `crates/ui-egui/src/panels/dialogs.rs`, `crates/text/src/fonts.rs` | Missing GemiTokens `Inferno Pixel Glass` dark/light themes (`infernoDark` / `infernoLight`); `About` dialog and `pick_open_project` still displayed `"FilmCraft"`; `GemiFontSans` directory not probed; Arabic (`ar`) missing from interface language choices. | Added `ThemeKind::InfernoDark` and `ThemeKind::InfernoLight` with canonical `gemi-core.v1.json` tokens while keeping `monitor_bg` and `plot_bg` strictly color-neutral (`R == G == B`); rebranded `About FilmTron` and file dialogs while preserving upstream credits; added `GemiFontSans` font discovery and Arabic (`ar`) language + RTL verification. |
| **FILM-005** | **High** | `crates/automation/src/gemimcp.rs`, `contracts/gemimcp-product-filmtron.v1.json` | No `gemimcp.product.v1` contract adapter or outbound WSS federation bridge for `GemiMCP` (`mcp.gemitron.net`). | Implemented `crates/automation/src/gemimcp.rs` exposing semantic `film.*` tools across all editing/grading/audio/render/UI domains + all 270+ `film.cmd.*` engine commands, `OutboundBridgeAdapter` (`wss://mcp.gemitron.net/v1/bridge/filmtron`), and exported `contracts/gemimcp-product-filmtron.v1.json`. |
| **FILM-006** | **Medium** | `crates/automation/src/gemiguard.rs` | No product-side consumer contract for `auth.gemitron.net` OIDC PKCE (`S256`) login or `GemiGuard` Ed25519 offline lease / revocation verification. | Implemented `crates/automation/src/gemiguard.rs` with OIDC PKCE (`S256`, `filmtron-desktop`), `filmtron:*` capability scopes, pure-Rust RFC 8032 Ed25519 signature verification for `gemiguard.offline-lease.v1` and `gemiguard.revocation-list.v1`, and non-destructive offline fallback (`preserveUserProjectData: true`). |
