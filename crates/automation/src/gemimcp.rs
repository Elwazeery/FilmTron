//! GemiMCP federated product manifest, semantic `film.*` tool catalog, 1:1 `film.cmd.*`
//! native engine command schemas, and outbound-only desktop bridge adapter (`FILM-005`).
//!
//! Architecture invariants (`docs/01-FORK-PRODUCT-AND-CLOUD-MCP-CHARTER.md`):
//! - `GemiTron88/GemiMCP` is the sole cloud MCP server owner and federation authority.
//! - FilmTron runs locally on the user's workstation and connects outbound-only to GemiMCP
//!   (`wss://mcp.gemitron.net/v1/bridge/filmtron`) or via an authenticated local GemiMCP relay.
//! - Zero public local TCP listeners are opened (`127.0.0.1` + per-launch 256-bit token only).
//! - Every mutating command acts on exact native object IDs (`ItemId`, `ClipId`, track indices)
//!   and rational `Tick` (`254,016,000,000` ticks/sec) positions, never approximate coordinates,
//!   and supports `expectedRevision` + `idempotencyKey` with authoritative `revision` readback.

use std::path::Path;

use filmcraft_engine::commands::CommandSpec;
use filmcraft_engine::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::security::{AuthorizedWorkspace, IdempotencyCache, execute_guarded};

pub const SCHEMA_VERSION: &str = "gemimcp.product-manifest.v1";
pub const PRODUCT_ID: &str = "filmtron";
pub const PRODUCT_NAME: &str = "FilmTron";
pub const CLOUD_AUTHORITY: &str = "GemiTron88/GemiMCP";
pub const DEFAULT_CLOUD_WS_ENDPOINT: &str = "wss://mcp.gemitron.net/v1/bridge/filmtron";

pub const SCOPE_READ: &str = "filmtron:read";
pub const SCOPE_EDIT: &str = "filmtron:edit";
pub const SCOPE_RENDER: &str = "filmtron:render";
pub const SCOPE_UI: &str = "filmtron:ui";

/// Permission scope descriptor registered with Gemi Access (`auth.gemitron.net`) and GemiMCP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionScope {
    pub scope: String,
    pub description: String,
    pub mutating: bool,
}

/// Outbound-only federation transport contract between the local FilmTron Desktop Agent and GemiMCP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationEndpointConfig {
    pub cloud_authority: String,
    pub websocket_endpoint: String,
    pub outbound_only: bool,
    pub no_public_local_listeners: bool,
    pub local_control_bind: String,
    pub requires_per_launch_token_auth: bool,
    pub timebase_ticks_per_second: i64,
}

impl Default for FederationEndpointConfig {
    fn default() -> Self {
        Self {
            cloud_authority: CLOUD_AUTHORITY.into(),
            websocket_endpoint: DEFAULT_CLOUD_WS_ENDPOINT.into(),
            outbound_only: true,
            no_public_local_listeners: true,
            local_control_bind: "127.0.0.1".into(),
            requires_per_launch_token_auth: true,
            timebase_ticks_per_second: filmcraft_time::TICKS_PER_SECOND,
        }
    }
}

/// A tool schema entry in the GemiMCP product manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GemiMcpToolSchema {
    pub name: String,
    pub family: String,
    pub engine_command: String,
    pub description: String,
    pub required_scope: String,
    pub mutating: bool,
    pub supports_revision_fence: bool,
    pub supports_idempotency_key: bool,
    pub input_schema: Value,
}

/// Complete GemiMCP product manifest for FilmTron (`contracts/gemimcp-product-filmtron.v1.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductManifest {
    pub schema_version: String,
    pub product_id: String,
    pub product_name: String,
    pub version: String,
    pub federation: FederationEndpointConfig,
    pub scopes: Vec<PermissionScope>,
    pub semantic_tool_count: usize,
    pub engine_command_tool_count: usize,
    pub semantic_tools: Vec<GemiMcpToolSchema>,
    pub engine_command_tools: Vec<GemiMcpToolSchema>,
}

/// Canonical permission scopes for FilmTron tools.
pub fn permission_scopes() -> Vec<PermissionScope> {
    vec![
        PermissionScope {
            scope: SCOPE_READ.into(),
            description: "Inspect projects, bins, sequences, rational Tick timebases, frames, and scopes without mutating state.".into(),
            mutating: false,
        },
        PermissionScope {
            scope: SCOPE_EDIT.into(),
            description: "Mutate project bins, sequences, timeline clips, trims, transitions, effects, keyframes, titles, captions, color, and audio.".into(),
            mutating: true,
        },
        PermissionScope {
            scope: SCOPE_RENDER.into(),
            description: "Configure export presets, render frames, enqueue export jobs, and export media/interchange files within authorized workspace roots.".into(),
            mutating: true,
        },
        PermissionScope {
            scope: SCOPE_UI.into(),
            description: "Switch workspaces, panels, themes, languages, and inspect live desktop UI state.".into(),
            mutating: true,
        },
    ]
}

fn is_render_command(id: &str) -> bool {
    id.starts_with("export.")
        || matches!(
            id,
            "file.exportMedia"
                | "file.exportFrame"
                | "file.exportAaf"
                | "file.exportOmf"
                | "file.exportEdl"
                | "file.exportFcpxml"
                | "file.exportAudio"
                | "captions.export"
        )
}

fn command_family(id: &str) -> &'static str {
    let prefix = id.split('.').next().unwrap_or("general");
    match prefix {
        "file" | "project" | "media" | "mediaBrowser" | "mediaCache" => "projects_and_media",
        "sequence" | "playhead" | "source" | "multicam" => "sequences_and_timebases",
        "timeline" | "clip" | "edit" | "markers" => "edits_trims_and_clips",
        "effects" | "transition" | "presets" => "effects_and_transitions",
        "keyframes" => "keyframes_and_properties",
        "graphics" | "captions" => "titles_and_captions",
        "color" => "color_and_scopes",
        "audio" | "mixer" | "essentialSound" | "voiceover" => "audio_and_dsp",
        "export" => "render_and_export",
        "prefs" | "shortcuts" | "window" | "view" | "app" => "ui_and_workspace",
        _ => "engine",
    }
}

fn build_input_schema(params_hint: &str, mutating: bool) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    let trimmed = params_hint.trim().trim_start_matches('{').trim_end_matches('}');
    if !trimmed.is_empty() {
        for field in trimmed.split(',') {
            if let Some((raw_name, raw_ty)) = field.split_once(':') {
                let raw_name = raw_name.trim();
                let optional = raw_name.ends_with('?');
                let name = raw_name.trim_end_matches('?').trim();
                if name.is_empty() {
                    continue;
                }
                let ty = raw_ty.trim();
                let prop = match ty {
                    "int" | "i64" | "u64" | "usize" => json!({"type": "integer", "description": ty}),
                    "float" | "f64" | "f32" | "number" => json!({"type": "number", "description": ty}),
                    "bool" | "boolean" => json!({"type": "boolean", "description": ty}),
                    t if t.starts_with('[') => json!({"type": "array", "description": ty}),
                    _ => json!({"type": "string", "description": ty}),
                };
                properties.insert(name.to_string(), prop);
                if !optional {
                    required.push(Value::String(name.to_string()));
                }
            }
        }
    }
    if mutating {
        properties.insert(
            "expectedRevision".into(),
            json!({
                "type": "integer",
                "minimum": 0,
                "description": "Optional optimistic concurrency fence matching Session::revision."
            }),
        );
        properties.insert(
            "idempotencyKey".into(),
            json!({
                "type": "string",
                "description": "Optional client-generated idempotency key for safe retry reconciliation."
            }),
        );
    }
    json!({
        "type": "object",
        "description": params_hint,
        "properties": properties,
        "required": required,
        "additionalProperties": true
    })
}

fn spec_to_tool_schema(spec: &CommandSpec) -> GemiMcpToolSchema {
    let mutating = spec.journal;
    let required_scope = if !mutating {
        SCOPE_READ
    } else if is_render_command(spec.id) {
        SCOPE_RENDER
    } else {
        SCOPE_EDIT
    };
    let menu_path = if spec.menu.is_empty() {
        String::new()
    } else {
        format!(" ({})", spec.menu.join(" > "))
    };
    GemiMcpToolSchema {
        name: format!("film.cmd.{}", spec.id),
        family: command_family(spec.id).into(),
        engine_command: spec.id.into(),
        description: format!("{}{}", spec.label, menu_path),
        required_scope: required_scope.into(),
        mutating,
        supports_revision_fence: mutating,
        supports_idempotency_key: mutating,
        input_schema: build_input_schema(spec.params, mutating),
    }
}

/// High-level semantic `film.*` tools covering every required editing capability family.
pub fn semantic_tools() -> Vec<GemiMcpToolSchema> {
    let defs: &[(&str, &str, &str, &str, &str, bool)] = &[
        ("film.project.new", "projects_and_media", "file.newProject", "Create a new empty FilmTron project (.fcproj).", SCOPE_EDIT, true),
        ("film.project.open", "projects_and_media", "file.open", "Open an existing .fcproj project within an authorized workspace root.", SCOPE_EDIT, true),
        ("film.project.save", "projects_and_media", "file.saveAs", "Atomically save the current project to disk and verify roundtrip.", SCOPE_EDIT, true),
        ("film.project.recover", "projects_and_media", "file.recover", "Recover unsaved changes from the crash-recovery journal.", SCOPE_EDIT, true),
        ("film.media.import", "projects_and_media", "file.import", "Import video, audio, or still image media files into a project bin.", SCOPE_EDIT, true),
        ("film.media.relink", "projects_and_media", "media.relink", "Relink an offline or moved media item by exact ItemId.", SCOPE_EDIT, true),
        ("film.media.new_bin", "projects_and_media", "file.newBin", "Create a media bin in the project library.", SCOPE_EDIT, true),
        ("film.sequence.create", "sequences_and_timebases", "file.newSequence", "Create a frame-accurate sequence with rational Tick timebase.", SCOPE_EDIT, true),
        ("film.sequence.settings", "sequences_and_timebases", "sequence.settings", "Modify sequence resolution, rational frame rate, PAR, and audio sample rate.", SCOPE_EDIT, true),
        ("film.sequence.add_tracks", "sequences_and_timebases", "sequence.addTracks", "Add video, audio, or submix tracks to the active sequence.", SCOPE_EDIT, true),
        ("film.timeline.place_clip", "edits_trims_and_clips", "timeline.place", "Place a media item onto a video/audio track at an exact Tick position.", SCOPE_EDIT, true),
        ("film.timeline.razor", "edits_trims_and_clips", "sequence.addEdit", "Split clip(s) at the exact playhead or specified rational Tick.", SCOPE_EDIT, true),
        ("film.timeline.trim", "edits_trims_and_clips", "timeline.trim", "Trim a clip's in/out point by exact ClipId and Tick delta.", SCOPE_EDIT, true),
        ("film.timeline.ripple_trim", "edits_trims_and_clips", "trim.edit", "Ripple-trim a clip edge and shift downstream clips frame-accurately.", SCOPE_EDIT, true),
        ("film.timeline.roll_trim", "edits_trims_and_clips", "timeline.roll", "Roll-trim an edit point between two adjacent clips.", SCOPE_EDIT, true),
        ("film.timeline.slip", "edits_trims_and_clips", "timeline.slip", "Slip a clip's source in/out window without moving its timeline position.", SCOPE_EDIT, true),
        ("film.timeline.slide", "edits_trims_and_clips", "timeline.slide", "Slide a clip between its neighbors while preserving combined duration.", SCOPE_EDIT, true),
        ("film.timeline.add_marker", "edits_trims_and_clips", "markers.add", "Add a timeline or clip marker at an exact Tick.", SCOPE_EDIT, true),
        ("film.effects.apply", "effects_and_transitions", "effects.apply", "Apply a video or audio effect to a clip by ClipId.", SCOPE_EDIT, true),
        ("film.effects.set_param", "effects_and_transitions", "effects.setParam", "Set an effect parameter value on a clip by ClipId and effect index.", SCOPE_EDIT, true),
        ("film.transitions.apply", "effects_and_transitions", "sequence.applyVideoTransition", "Apply a video transition at a clip edge.", SCOPE_EDIT, true),
        ("film.keyframes.set", "keyframes_and_properties", "effects.setKeyframe", "Insert or update a property/effect keyframe at an exact Tick.", SCOPE_EDIT, true),
        ("film.titles.add_text", "titles_and_captions", "graphics.newText", "Create a vector title/text graphic layer (supports Arabic/RTL and GemiFontSans).", SCOPE_EDIT, true),
        ("film.captions.add", "titles_and_captions", "captions.add", "Add a caption cue on a caption track with exact start/end Tick.", SCOPE_EDIT, true),
        ("film.color.set_lumetri", "color_and_scopes", "lumetri.applyPreset", "Apply or adjust Lumetri color grading presets and parameters on a clip.", SCOPE_EDIT, true),
        ("film.audio.set_gain", "audio_and_dsp", "clip.audioGain", "Set or normalize clip audio gain in dB.", SCOPE_EDIT, true),
        ("film.audio.set_track_mixer", "audio_and_dsp", "mixer.setValue", "Adjust audio track mixer fader level or pan.", SCOPE_EDIT, true),
        ("film.render.export_media", "render_and_export", "file.exportMedia", "Export the active sequence to MP4 (H.264/AAC), MOV, WAV, or image sequence.", SCOPE_RENDER, true),
        ("film.render.export_frame", "render_and_export", "file.exportFrame", "Render and export a single Program/Source frame to PNG.", SCOPE_RENDER, true),
        ("film.history.undo", "undo_redo_and_recovery", "edit.undo", "Undo the last mutating edit command.", SCOPE_EDIT, true),
        ("film.history.redo", "undo_redo_and_recovery", "edit.redo", "Redo the last undone edit command.", SCOPE_EDIT, true),
        ("film.workspace.set_theme", "ui_and_workspace", "prefs.set", "Update appearance/theme preferences (including Inferno Pixel Glass Dark/Light).", SCOPE_UI, true),
    ];

    let specs = filmcraft_engine::command_specs();
    defs.iter()
        .map(|(name, family, cmd_id, desc, scope, mutating)| {
            let params_hint = specs.iter().find(|s| s.id == *cmd_id).map(|s| s.params).unwrap_or("{}");
            GemiMcpToolSchema {
                name: (*name).into(),
                family: (*family).into(),
                engine_command: (*cmd_id).into(),
                description: (*desc).into(),
                required_scope: (*scope).into(),
                mutating: *mutating,
                supports_revision_fence: *mutating,
                supports_idempotency_key: *mutating,
                input_schema: build_input_schema(params_hint, *mutating),
            }
        })
        .collect()
}

/// 1:1 tool schemas for every native engine command in `filmcraft_engine::command_specs()`.
pub fn engine_command_tools() -> Vec<GemiMcpToolSchema> {
    filmcraft_engine::command_specs().iter().map(spec_to_tool_schema).collect()
}

/// Build the complete GemiMCP product manifest for FilmTron.
pub fn build_product_manifest() -> ProductManifest {
    let semantic = semantic_tools();
    let engine_tools = engine_command_tools();
    ProductManifest {
        schema_version: SCHEMA_VERSION.into(),
        product_id: PRODUCT_ID.into(),
        product_name: PRODUCT_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        federation: FederationEndpointConfig::default(),
        scopes: permission_scopes(),
        semantic_tool_count: semantic.len(),
        engine_command_tool_count: engine_tools.len(),
        semantic_tools: semantic,
        engine_command_tools: engine_tools,
    }
}

/// Write the canonical GemiMCP product manifest JSON to `path`.
pub fn write_product_manifest(path: &Path) -> Result<ProductManifest, String> {
    let manifest = build_product_manifest();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create manifest dir {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| format!("serialize manifest: {e}"))?;
    std::fs::write(path, format!("{json}\n")).map_err(|e| format!("write manifest {}: {e}", path.display()))?;
    Ok(manifest)
}

/// Incoming federated tool call envelope from `GemiTron88/GemiMCP`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederatedToolCall {
    pub call_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub arguments: Value,
    #[serde(default)]
    pub granted_scopes: Vec<String>,
}

/// Outbound-only desktop adapter that validates Gemi Access scopes, workspace path confinement,
/// revision fences, and idempotency keys before dispatching to the local FilmTron engine.
pub struct OutboundBridgeAdapter {
    manifest: ProductManifest,
    workspace: Option<AuthorizedWorkspace>,
    idempotency: IdempotencyCache,
}

impl OutboundBridgeAdapter {
    pub fn new(workspace: Option<AuthorizedWorkspace>) -> Self {
        Self {
            manifest: build_product_manifest(),
            workspace,
            idempotency: IdempotencyCache::new(),
        }
    }

    /// Whether a trusted caller bound this adapter to authorized project roots.
    pub fn has_workspace_root(&self) -> bool { self.workspace.is_some() }

    pub fn manifest(&self) -> &ProductManifest {
        &self.manifest
    }

    pub fn resolve_tool(&self, tool_name: &str) -> Option<&GemiMcpToolSchema> {
        self.manifest
            .semantic_tools
            .iter()
            .chain(self.manifest.engine_command_tools.iter())
            .find(|t| t.name == tool_name || t.engine_command == tool_name)
    }

    /// Execute a federated GemiMCP tool call against a local `Session` with full scope,
    /// workspace path, revision-fence, and idempotency enforcement.
    pub fn execute_on_session(&mut self, session: &mut Session, call: &FederatedToolCall) -> Value {
        let Some(tool) = self.resolve_tool(&call.tool_name).cloned() else {
            return json!({
                "callId": call.call_id,
                "ok": false,
                "code": "UNKNOWN_TOOL",
                "error": format!("unknown FilmTron MCP tool `{}`", call.tool_name),
                "revision": session.revision
            });
        };

        if !call.granted_scopes.iter().any(|s| s == &tool.required_scope || s == "filmtron:*") {
            return json!({
                "callId": call.call_id,
                "ok": false,
                "code": "SCOPE_DENIED",
                "requiredScope": tool.required_scope,
                "error": format!("tool `{}` requires scope `{}`", tool.name, tool.required_scope),
                "revision": session.revision
            });
        }

        let engine_cmd = tool.engine_command.clone();
        match execute_guarded(
            session,
            &mut self.idempotency,
            self.workspace.as_ref(),
            &engine_cmd,
            call.arguments.clone(),
        ) {
            Ok(result) => {
                let idempotent_replay = result.get("idempotentReplay").and_then(Value::as_bool).unwrap_or(false);
                json!({
                    "callId": call.call_id,
                    "tool": tool.name,
                    "ok": true,
                    "result": result,
                    "revision": session.revision,
                    "idempotentReplay": idempotent_replay,
                })
            }
            Err(err) => {
                let msg = err.to_string();
                let code = if msg.contains("REVISION_CONFLICT") {
                    "REVISION_CONFLICT"
                } else if msg.contains("IDEMPOTENCY_CONFLICT") {
                    "IDEMPOTENCY_CONFLICT"
                } else if msg.contains("ACCESS_DENIED") || msg.contains("traversal") || msg.contains("device name") {
                    "PATH_OUTSIDE_WORKSPACE"
                } else {
                    "COMMAND_FAILED"
                };
                json!({
                    "callId": call.call_id,
                    "tool": tool.name,
                    "ok": false,
                    "code": code,
                    "error": msg,
                    "revision": session.revision,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_covers_every_engine_command_and_semantic_family() {
        let manifest = build_product_manifest();
        assert_eq!(manifest.schema_version, SCHEMA_VERSION);
        assert_eq!(manifest.product_id, PRODUCT_ID);
        assert!(manifest.federation.outbound_only);
        assert!(manifest.federation.no_public_local_listeners);
        assert_eq!(manifest.engine_command_tool_count, filmcraft_engine::command_specs().len());
        assert!(manifest.semantic_tool_count >= 30);

        for spec in filmcraft_engine::command_specs() {
            assert!(
                manifest.engine_command_tools.iter().any(|t| t.engine_command == spec.id),
                "missing engine command in GemiMCP manifest: {}",
                spec.id
            );
        }

        let specs = filmcraft_engine::command_specs();
        for sem in &manifest.semantic_tools {
            assert!(
                specs.iter().any(|s| s.id == sem.engine_command),
                "semantic tool {} references non-existent engine_command {}",
                sem.name,
                sem.engine_command
            );
        }

        let contract_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .join("contracts/gemimcp-product-filmtron.v1.json");
        let written = write_product_manifest(&contract_path).expect("write product manifest");
        assert_eq!(written.engine_command_tool_count, manifest.engine_command_tool_count);
    }

    #[test]
    fn outbound_adapter_enforces_scopes_revision_fences_and_workspace_confinement() {
        let tmp = std::env::temp_dir().join(format!("filmtron-gemimcp-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let ws = AuthorizedWorkspace::new([tmp.clone()]).expect("workspace");
        let mut adapter = OutboundBridgeAdapter::new(Some(ws));
        let mut session = Session::default();

        // 1. Missing scope -> SCOPE_DENIED
        let denied = adapter.execute_on_session(
            &mut session,
            &FederatedToolCall {
                call_id: "c1".into(),
                tool_name: "film.media.new_bin".into(),
                arguments: json!({"name": "Rushes"}),
                granted_scopes: vec![SCOPE_READ.into()],
            },
        );
        assert_eq!(denied["ok"], false);
        assert_eq!(denied["code"], "SCOPE_DENIED");

        // 2. Authorized call with expectedRevision + idempotencyKey -> succeeds and increments revision
        let rev0 = session.revision;
        let ok = adapter.execute_on_session(
            &mut session,
            &FederatedToolCall {
                call_id: "c2".into(),
                tool_name: "film.media.new_bin".into(),
                arguments: json!({
                    "name": "Rushes",
                    "expectedRevision": rev0,
                    "idempotencyKey": "idem-bin-1"
                }),
                granted_scopes: vec![SCOPE_EDIT.into()],
            },
        );
        assert_eq!(ok["ok"], true, "{ok}");
        let rev1 = ok["revision"].as_u64().expect("revision");
        assert!(rev1 > rev0);

        // 3. Idempotent replay with same key & payload -> returns cached result without re-mutating
        let replay = adapter.execute_on_session(
            &mut session,
            &FederatedToolCall {
                call_id: "c3".into(),
                tool_name: "film.media.new_bin".into(),
                arguments: json!({
                    "name": "Rushes",
                    "expectedRevision": rev0,
                    "idempotencyKey": "idem-bin-1"
                }),
                granted_scopes: vec![SCOPE_EDIT.into()],
            },
        );
        assert_eq!(replay["ok"], true);
        assert_eq!(replay["idempotentReplay"], true);
        assert_eq!(session.revision, rev1);

        // 4. Stale expectedRevision -> REVISION_CONFLICT
        let stale = adapter.execute_on_session(
            &mut session,
            &FederatedToolCall {
                call_id: "c4".into(),
                tool_name: "film.media.new_bin".into(),
                arguments: json!({
                    "name": "Audio",
                    "expectedRevision": rev0
                }),
                granted_scopes: vec![SCOPE_EDIT.into()],
            },
        );
        assert_eq!(stale["ok"], false);
        assert_eq!(stale["code"], "REVISION_CONFLICT");

        // 5. Out-of-workspace path -> PATH_OUTSIDE_WORKSPACE
        let outside = adapter.execute_on_session(
            &mut session,
            &FederatedToolCall {
                call_id: "c5".into(),
                tool_name: "film.project.open".into(),
                arguments: json!({
                    "path": "C:\\Windows\\System32\\cmd.exe"
                }),
                granted_scopes: vec![SCOPE_EDIT.into()],
            },
        );
        assert_eq!(outside["ok"], false);
        assert_eq!(outside["code"], "PATH_OUTSIDE_WORKSPACE");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
