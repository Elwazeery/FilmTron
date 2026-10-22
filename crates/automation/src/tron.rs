//! FilmTron TRON creative plan gate and canonical native executor.
//!
//! This module does not load a model or create an MCP server. All generated
//! plans are untrusted. An owner-authorized caller constructs TrustedFilmScope
//! from the REAL active editor session, current Gemi Access grants and user
//! approval; the model cannot supply this scope.
use std::collections::BTreeSet;

use filmcraft_engine::{
    Session,
    project::{ClipId, ItemId, ItemKind, Label, ProjectItem, TrackKind},
};
use filmcraft_time::{FrameRate, Tick, TICKS_PER_SECOND};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::gemimcp::{FederatedToolCall, OutboundBridgeAdapter, semantic_tools};

pub const PLAN_SCHEMA: &str = "filmtron.tron.plan.v1";
pub const MAX_STEPS: usize = 12;
const MAX_PARAMS_BYTES: usize = 65_536;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilmTronPlan {
    pub schema: String,
    pub product: String,
    pub project_id: String,
    pub plan_id: String,
    pub expected_revision: u64,
    pub sequence_id: Option<u64>,
    pub fps_num: i64,
    pub fps_den: i64,
    pub steps: Vec<FilmTronStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilmTronStep {
    pub id: String,
    pub tool: String,
    pub params: Value,
}

/// Trusted only when constructed by the native authenticated caller.
/// Do NOT deserialize model output into this authorization structure.
#[derive(Clone, Debug)]
pub struct TrustedFilmScope {
    pub project_id: String,
    pub current_revision: u64,
    pub sequence_id: Option<u64>,
    pub fps_num: i64,
    pub fps_den: i64,
    pub granted_scopes: BTreeSet<String>,
    pub allowed_tools: BTreeSet<String>,
    pub allow_mutations: bool,
    pub approved_external_writes: bool,
}

fn reject(reason: &str) -> String {
    format!("FILM_TRON_PREFLIGHT_DENIED: {reason}")
}

fn safe_identifier(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// These operations can touch the filesystem, rebuild timelines, or produce
/// external media. They require a separate owner approval, not a model flag.
fn is_file_operation(tool: &str) -> bool {
    matches!(tool,
        "film.project.open" | "film.project.save" | "film.media.import"
        | "film.media.relink" | "film.render.export_media" | "film.render.export_frame")
}

fn needs_separate_approval(tool: &str) -> bool {
    matches!(tool,
        "film.project.new" | "film.project.open" | "film.project.save" | "film.project.recover"
        | "film.media.import" | "film.media.relink" | "film.render.export_media"
        | "film.render.export_frame" | "film.sequence.settings"
    )
}

/// Reject fractional seconds and timecode strings in frame-aligned native
/// fields. FilmTron's engine uses integral 254_016_000_000 ticks per second.
/// FrameRate::tick_of is authoritative, including 30000/1001 drop-frame-rate
/// math; a float conversion would introduce drift.
fn verify_frame_boundaries(value: &Value, rate: FrameRate) -> Result<(), String> {
    const FIELDS: &[&str] = &[
        "startTick", "endTick", "positionTick", "atTick",
        "inTick", "outTick", "durationTicks", "timelineTick"
    ];
    let Value::Object(props) = value else { return Err(reject("TOOL_PARAMS_NOT_OBJECT")); };
    for field in FIELDS {
        if let Some(v) = props.get(*field) {
            let tick = v.as_i64().ok_or_else(|| reject("TICK_MUST_BE_SIGNED_INTEGER"))?;
            if tick < 0 && *field != "timelineTick" {
                return Err(reject("NEGATIVE_TIMELINE_TICK"));
            }
            let native = Tick(tick);
            if rate.tick_of(rate.frame_at(native)) != native {
                return Err(reject("FRAME_BOUNDARY_REQUIRED"));
            }
        }
    }
    Ok(())
}

pub fn preflight(plan: &FilmTronPlan, scope: &TrustedFilmScope) -> Result<Value, String> {
    if plan.schema != PLAN_SCHEMA || plan.product != "filmtron" {
        return Err(reject("SCHEMA_OR_PRODUCT_MISMATCH"));
    }
    if plan.project_id != scope.project_id || plan.project_id.trim().is_empty()
        || plan.sequence_id != scope.sequence_id
    {
        return Err(reject("PROJECT_OR_SEQUENCE_MISMATCH"));
    }
    if plan.expected_revision != scope.current_revision {
        return Err(reject("REVISION_CONFLICT"));
    }
    if !safe_identifier(&plan.plan_id) || plan.steps.is_empty() || plan.steps.len() > MAX_STEPS {
        return Err(reject("PLAN_ID_OR_STEP_BUDGET"));
    }
    if plan.fps_num <= 0 || plan.fps_den <= 0
        || plan.fps_num != scope.fps_num || plan.fps_den != scope.fps_den
    {
        return Err(reject("TIMEBASE_MISMATCH"));
    }
    let rate = FrameRate::new(plan.fps_num, plan.fps_den);
    // An overflow-prone/exotic rate must not silently round frame boundaries.
    if rate.frame_duration().0 <= 0 {
        return Err(reject("INVALID_FRAME_DURATION"));
    }

    let catalog = semantic_tools();
    let mut ids = BTreeSet::new();
    let mut writes = 0_u64;
    let mut approvals = 0_u64;
    for step in &plan.steps {
        if !safe_identifier(&step.id) || !ids.insert(step.id.as_str()) {
            return Err(reject("DUPLICATE_OR_INVALID_STEP_ID"));
        }
        if !scope.allowed_tools.contains(&step.tool) {
            return Err(reject("UNGRANTED_TOOL"));
        }
        // Only canonical high-level film.* tools. No film.cmd.*, raw engine
        // commands, arbitrary scripting, shell or guessed capabilities.
        let native = catalog.iter().find(|c| c.name == step.tool)
            .ok_or_else(|| reject("UNSUPPORTED_SEMANTIC_TOOL"))?;
        if !filmcraft_engine::command_specs().iter().any(|c| c.id == native.engine_command) {
            return Err(reject("MISSING_NATIVE_HANDLER"));
        }
        if !scope.granted_scopes.contains(&native.required_scope) {
            return Err(reject("MISSING_GEMI_ACCESS_SCOPE"));
        }
        if !step.params.is_object() {
            return Err(reject("TOOL_PARAMS_NOT_OBJECT"));
        }
        let encoded = serde_json::to_vec(&step.params).map_err(|_| reject("INVALID_JSON"))?;
        if encoded.len() > MAX_PARAMS_BYTES {
            return Err(reject("ARGUMENT_BUDGET_EXCEEDED"));
        }
        for field in [
            "projectId", "sequenceId", "deviceId", "expectedRevision",
            "idempotencyKey", "accessToken", "bearerToken", "confirmationToken",
            "workspaceRoots", "grantedScopes", "approvalReceiptId"
        ] {
            if step.params.get(field).is_some() {
                return Err(reject("MODEL_TRIED_TO_OVERRIDE_TRUSTED_SCOPE"));
            }
        }
        verify_frame_boundaries(&step.params, rate)?;
        if native.mutating && !scope.allow_mutations {
            return Err(reject("READ_ONLY_OR_NO_LOCAL_APPROVAL"));
        }
        if native.mutating { writes += 1; }
        if needs_separate_approval(&step.tool) {
            approvals += 1;
            if !scope.approved_external_writes {
                return Err(reject("SEPARATE_APPROVAL_REQUIRED"));
            }
        }
    }
    Ok(json!({
        "schema": "filmtron.tron.preflight.v1",
        "status": "PREPARED_NOT_EXECUTED",
        "projectId": scope.project_id,
        "sequenceId": scope.sequence_id,
        "expectedRevision": scope.current_revision,
        "timebaseTicksPerSecond": TICKS_PER_SECOND,
        "fps": {"num": scope.fps_num, "den": scope.fps_den},
        "stepCount": plan.steps.len(),
        "writeCount": writes,
        "additionalApprovalCount": approvals,
        "applied": false
    }))
}

fn resolve_item_id(session: &Session, raw: Option<&Value>, want_audio_only: bool) -> Option<u64> {
    let media_items: Vec<(u64, &ProjectItem)> = session
        .project
        .items
        .iter()
        .filter(|(_, it)| !matches!(it.kind, ItemKind::Sequence(_)))
        .map(|(id, it)| (id.0, it))
        .collect();
    if let Some(v) = raw {
        if let Some(id) = v.as_u64() {
            if session.project.item(ItemId(id)).is_some() {
                return Some(id);
            }
        }
        if let Some(s) = v.as_str() {
            let trimmed = s.trim();
            if let Ok(id) = trimmed.parse::<u64>() {
                if session.project.item(ItemId(id)).is_some() {
                    return Some(id);
                }
            }
            for (_, it) in &media_items {
                if it.name.eq_ignore_ascii_case(trimmed) {
                    return Some(it.id.0);
                }
            }
            let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
            if let Ok(idx) = digits.parse::<usize>() {
                let zero_idx = idx.saturating_sub(1);
                if let Some((id, _)) = media_items.get(zero_idx).or_else(|| media_items.get(idx)) {
                    return Some(*id);
                }
            }
        }
    }
    if want_audio_only {
        if let Some((id, _)) = media_items.iter().find(|(_, it)| !it.has_video() && it.has_audio()) {
            return Some(*id);
        }
    }
    media_items.first().map(|(id, _)| *id)
}

fn resolve_clip_id(session: &Session, raw: Option<&Value>, want_audio: bool) -> Option<u64> {
    let seq = session.active_sequence()?;
    let mut video_clips = Vec::new();
    let mut graphic_clips = Vec::new();
    for tr in &seq.video_tracks {
        for it in &tr.items {
            if it.effects.iter().any(|e| e.effect.starts_with("graphic")) {
                graphic_clips.push(it.id.0);
            } else {
                video_clips.push(it.id.0);
            }
        }
    }
    video_clips.extend(graphic_clips);
    let mut audio_clips = Vec::new();
    for tr in &seq.audio_tracks {
        for it in &tr.items {
            audio_clips.push(it.id.0);
        }
    }
    let pool = if want_audio { &audio_clips } else { &video_clips };
    if let Some(v) = raw {
        if let Some(id) = v.as_u64() {
            if seq.find_item(ClipId(id)).is_some() {
                return Some(id);
            }
        }
        if let Some(s) = v.as_str() {
            let trimmed = s.trim();
            if let Ok(id) = trimmed.parse::<u64>() {
                if seq.find_item(ClipId(id)).is_some() {
                    return Some(id);
                }
            }
            let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
            if let Ok(idx) = digits.parse::<usize>() {
                let zero_idx = idx.saturating_sub(1);
                if let Some(id) = pool.get(zero_idx).or_else(|| pool.get(idx)) {
                    return Some(*id);
                }
            }
        }
    }
    for sel in &session.state.selection {
        if let Some((tid, _)) = seq.find_item(*sel) {
            let is_aud = seq.track(tid).is_some_and(|t| t.kind == TrackKind::Audio);
            if is_aud == want_audio {
                return Some(sel.0);
            }
        }
    }
    pool.first().copied().or_else(|| video_clips.first().copied()).or_else(|| audio_clips.first().copied())
}

fn normalize_marker_color(raw: &str) -> &'static str {
    for l in Label::ALL {
        if l.name().eq_ignore_ascii_case(raw.trim()) {
            return l.name();
        }
    }
    match raw.trim().to_ascii_lowercase().as_str() {
        "amber" | "orange" | "gold" => "Mango",
        "emerald" | "lime" => "Green",
        "cyan" | "sky" | "aqua" => "Cerulean",
        "red" | "crimson" | "pink" => "Rose",
        "indigo" => "Iris",
        _ => "Green",
    }
}

fn normalize_lumetri_preset(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "warm" | "golden" | "golden dusk" | "sunset" => "Golden Dusk",
        "cool" | "steel" | "steel blue" | "cold" => "Steel Blue",
        "punch" | "hard sun" | "contrast" | "vibrant" => "Hard Sun",
        "night" | "night exterior" | "dark" => "Night Exterior",
        _ => "Teal Shadows, Warm Skin",
    }
}

fn copy_tick_alias(obj: &mut Map<String, Value>, target_key: &str, aliases: &[&str]) {
    if obj.contains_key(target_key) {
        return;
    }
    for alias in aliases {
        if let Some(v) = obj.get(*alias).and_then(Value::as_i64) {
            obj.insert(target_key.to_owned(), json!(v));
            return;
        }
    }
}

fn normalize_semantic_step_params(
    session: &Session,
    tool_name: &str,
    raw_params: &Value,
) -> (String, Value, Vec<(String, Value)>) {
    let mut obj = raw_params.as_object().cloned().unwrap_or_default();
    let mut follow_up: Vec<(String, Value)> = Vec::new();
    let mut effective_tool = tool_name.to_owned();

    match tool_name {
        "film.timeline.place_clip" => {
            let want_audio_only = obj.contains_key("audioTrack") && !obj.contains_key("track");
            let item_raw = obj.get("item").or_else(|| obj.get("itemId")).or_else(|| obj.get("mediaId"));
            if let Some(item_id) = resolve_item_id(session, item_raw, want_audio_only) {
                obj.insert("item".into(), json!(item_id));
            }
            copy_tick_alias(&mut obj, "time", &["startTick", "positionTick", "atTick", "timelineTick"]);
            copy_tick_alias(&mut obj, "sourceIn", &["inTick"]);
            copy_tick_alias(&mut obj, "duration", &["durationTicks"]);
            if let Some(tr_num) = obj.get("track").and_then(Value::as_u64) {
                let exists_as_id = session.active_sequence().is_some_and(|q| q.video_tracks.iter().any(|t| t.id.0 == tr_num));
                if !exists_as_id {
                    obj.insert("track".into(), json!(format!("V{}", tr_num + 1)));
                }
            }
            if let Some(atr_num) = obj.get("audioTrack").and_then(Value::as_u64) {
                let exists_as_id = session.active_sequence().is_some_and(|q| q.audio_tracks.iter().any(|t| t.id.0 == atr_num));
                if !exists_as_id {
                    obj.insert("audioTrack".into(), json!(format!("A{}", atr_num + 1)));
                }
            }
            if want_audio_only {
                obj.insert("video".into(), json!(false));
                obj.insert("audio".into(), json!(true));
            } else if !obj.contains_key("audioTrack") && !obj.contains_key("audio") {
                obj.insert("audio".into(), json!(false));
            }
        }
        "film.timeline.razor" => {
            copy_tick_alias(&mut obj, "time", &["positionTick", "atTick", "startTick", "timelineTick"]);
        }
        "film.timeline.trim" | "film.timeline.ripple_trim" => {
            if tool_name == "film.timeline.ripple_trim" {
                effective_tool = "film.timeline.trim".to_owned();
                obj.insert("mode".into(), json!("ripple"));
            }
            if !obj.contains_key("edge") {
                obj.insert("edge".into(), json!("out"));
            }
            let clip_raw = obj.get("clip").or_else(|| obj.get("clipId"));
            if let Some(cid) = resolve_clip_id(session, clip_raw, false) {
                obj.insert("clip".into(), json!(cid));
            }
            if !obj.contains_key("delta") && !obj.contains_key("deltaFrames") {
                obj.insert("deltaFrames".into(), json!(-15));
            }
        }
        "film.timeline.slip" | "film.timeline.slide" => {
            let clip_raw = obj.get("clip").or_else(|| obj.get("clipId"));
            if let Some(cid) = resolve_clip_id(session, clip_raw, false) {
                obj.insert("clip".into(), json!(cid));
            }
            if !obj.contains_key("delta") && !obj.contains_key("deltaFrames") {
                obj.insert("deltaFrames".into(), json!(10));
            }
        }
        "film.timeline.add_marker" => {
            copy_tick_alias(&mut obj, "time", &["positionTick", "atTick", "startTick", "timelineTick"]);
            if let Some(c) = obj.get("color").and_then(Value::as_str).map(normalize_marker_color) {
                obj.insert("color".into(), json!(c));
            }
        }
        "film.transitions.apply" => {
            let clip_raw = obj.get("clip").or_else(|| obj.get("clipId"));
            if let Some(cid) = resolve_clip_id(session, clip_raw, false) {
                obj.insert("clip".into(), json!(cid));
            }
            if !obj.contains_key("edge") {
                obj.insert("edge".into(), json!("out"));
            }
        }
        "film.effects.apply" => {
            if !obj.contains_key("clips") {
                let clip_raw = obj.get("clip").or_else(|| obj.get("clipId"));
                if let Some(cid) = resolve_clip_id(session, clip_raw, false) {
                    obj.insert("clips".into(), json!([cid]));
                }
            }
            if !obj.contains_key("effect") {
                obj.insert("effect".into(), json!("gaussian_blur"));
            }
        }
        "film.effects.set_param" | "film.keyframes.set" => {
            let clip_raw = obj.get("clip").or_else(|| obj.get("clipId"));
            if let Some(cid) = resolve_clip_id(session, clip_raw, false) {
                obj.insert("clip".into(), json!(cid));
            }
            if !obj.contains_key("effect") {
                obj.insert("effect".into(), json!("motion"));
            }
            if !obj.contains_key("param") {
                obj.insert("param".into(), json!("scale"));
            }
            copy_tick_alias(&mut obj, "time", &["atTick", "positionTick", "startTick", "timelineTick"]);
            if tool_name == "film.keyframes.set" && !obj.contains_key("mediaTime") {
                let mt = obj.get("time").and_then(Value::as_i64).unwrap_or(0);
                obj.insert("mediaTime".into(), json!(mt));
            }
        }
        "film.color.set_lumetri" => {
            let preset_raw = obj.get("name").or_else(|| obj.get("preset")).and_then(Value::as_str).unwrap_or("Cinema");
            obj.insert("name".into(), json!(normalize_lumetri_preset(preset_raw)));
            let mut target_clips = Vec::new();
            if let Some(arr) = obj.get("clips").and_then(Value::as_array) {
                for item in arr {
                    if let Some(cid) = resolve_clip_id(session, Some(item), false) {
                        if !target_clips.contains(&cid) {
                            target_clips.push(cid);
                        }
                    }
                }
            }
            if target_clips.is_empty() {
                if let Some(seq) = session.active_sequence() {
                    for tr in &seq.video_tracks {
                        for it in &tr.items {
                            if !it.effects.iter().any(|e| e.effect.starts_with("graphic.")) {
                                target_clips.push(it.id.0);
                            }
                        }
                    }
                }
            }
            if target_clips.is_empty() {
                if let Some(cid) = resolve_clip_id(session, obj.get("clip"), false) {
                    target_clips.push(cid);
                }
            }
            if !target_clips.is_empty() {
                obj.insert("clips".into(), json!(target_clips));
            }
            for (json_key, param_id) in [
                ("exposure", "exposure"),
                ("contrast", "contrast"),
                ("temperature", "temperature"),
                ("tint", "tint"),
                ("saturation", "saturation"),
                ("highlights", "highlights"),
                ("shadows", "shadows"),
                ("vibrance", "vibrance"),
                ("vignetteAmount", "vignette_amount"),
                ("vignette_amount", "vignette_amount"),
            ] {
                if let Some(val) = obj.get(json_key).and_then(Value::as_f64) {
                    for &cid in &target_clips {
                        follow_up.push((
                            "film.effects.set_param".into(),
                            json!({
                                "clip": cid,
                                "effect": "lumetri",
                                "param": param_id,
                                "value": val
                            }),
                        ));
                    }
                }
            }
        }
        "film.titles.add_text" => {
            copy_tick_alias(&mut obj, "time", &["startTick", "positionTick", "atTick", "timelineTick"]);
            let dur_ticks = obj
                .get("durationTicks")
                .and_then(Value::as_i64)
                .unwrap_or(TICKS_PER_SECOND * 3);
            if !obj.contains_key("seconds") {
                let secs = (dur_ticks as f64 / TICKS_PER_SECOND as f64).max(0.5);
                obj.insert("seconds".into(), json!(secs));
            }
            if let Some(sz) = obj.get("size").and_then(Value::as_f64) {
                obj.insert("size".into(), json!(sz.clamp(24.0, 52.0)));
            } else {
                obj.insert("size".into(), json!(48.0));
            }
            let t_tick = obj.get("time").and_then(Value::as_i64).unwrap_or(0);
            let existing_graphic_clip = session.active_sequence().and_then(|seq| {
                for tr in &seq.video_tracks {
                    for it in &tr.items {
                        if it.effects.iter().any(|e| e.effect.starts_with("graphic")) {
                            let st = it.start.0;
                            let en = it.end().0;
                            if t_tick < en && (t_tick + dur_ticks) > st {
                                return Some(it.id.0);
                            }
                        }
                    }
                }
                None
            });

            let mut props = Map::new();
            props.insert(
                "align".into(),
                json!(obj.get("align").and_then(Value::as_str).unwrap_or("center")),
            );
            props.insert(
                "shadow".into(),
                json!(obj.get("shadow").and_then(Value::as_bool).unwrap_or(true)),
            );
            if let Some(fc) = obj.get("fillColor").or_else(|| obj.get("fill_color")).and_then(Value::as_str) {
                props.insert("fill_color".into(), json!(fc));
            }
            if let Some(bg) = obj.get("background").and_then(Value::as_bool) {
                props.insert("background".into(), json!(bg));
            }
            if let Some(pos) = obj.get("position").cloned() {
                props.insert("position".into(), pos);
            }
            if let Some(sz) = obj.get("size").cloned() {
                props.insert("size".into(), sz);
            }

            if let Some(existing_id) = existing_graphic_clip {
                effective_tool = "film.cmd.graphics.setText".to_owned();
                let text_val = obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("Title")
                    .to_owned();
                obj.clear();
                obj.insert("clip".into(), json!(existing_id));
                obj.insert("layer".into(), json!(0));
                obj.insert("text".into(), json!(text_val));
                follow_up.push((
                    "film.cmd.graphics.set".into(),
                    json!({
                        "clip": existing_id,
                        "layer": 0,
                        "props": props,
                        "time": t_tick
                    }),
                ));
            } else {
                obj.insert("newClip".into(), json!(true));
                obj.remove("track");
                follow_up.push((
                    "film.cmd.graphics.set".into(),
                    json!({
                        "layer": 0,
                        "props": props,
                        "time": t_tick
                    }),
                ));
            }
        }
        "film.captions.add" => {
            copy_tick_alias(&mut obj, "time", &["startTick", "positionTick", "atTick", "timelineTick"]);
            if !obj.contains_key("durationSeconds") {
                if let Some(dur) = obj.get("durationTicks").and_then(Value::as_i64) {
                    let secs = (dur as f64 / TICKS_PER_SECOND as f64).max(0.5);
                    obj.insert("durationSeconds".into(), json!(secs));
                }
            }
        }
        "film.audio.set_gain" => {
            if !obj.contains_key("db") {
                if let Some(g) = obj.get("gainDb").or_else(|| obj.get("gain")).and_then(Value::as_f64) {
                    obj.insert("db".into(), json!(g));
                } else {
                    obj.insert("db".into(), json!(-3.0));
                }
            }
            let mut target_clips = Vec::new();
            if let Some(arr) = obj.get("clips").and_then(Value::as_array) {
                for item in arr {
                    if let Some(cid) = resolve_clip_id(session, Some(item), true) {
                        if !target_clips.contains(&cid) {
                            target_clips.push(cid);
                        }
                    }
                }
            }
            if target_clips.is_empty() {
                if let Some(cid) = resolve_clip_id(session, obj.get("clip").or_else(|| obj.get("clipId")), true) {
                    target_clips.push(cid);
                }
            }
            if !target_clips.is_empty() {
                obj.insert("clips".into(), json!(target_clips));
            }
        }
        "film.audio.set_track_mixer" => {
            if !obj.contains_key("strip") {
                let st = obj.get("track").or_else(|| obj.get("trackId")).and_then(Value::as_str).unwrap_or("A1");
                obj.insert("strip".into(), json!(st));
            }
            if !obj.contains_key("lane") {
                obj.insert("lane".into(), json!("volume"));
            }
            if !obj.contains_key("value") {
                let v = obj.get("db").or_else(|| obj.get("gainDb")).and_then(Value::as_f64).unwrap_or(-3.0);
                obj.insert("value".into(), json!(v));
            }
        }
        _ => {}
    }

    (effective_tool, Value::Object(obj), follow_up)
}

/// A trusted native caller must repeat the project/revision check immediately
/// before entering this function and record the authoritative native receipts.
/// This executor enforces native permission/revision/idempotency for EACH step.
/// It never invents a successful receipt after partial failure.
pub fn execute_checked(
    session: &mut Session,
    adapter: &mut OutboundBridgeAdapter,
    plan: &FilmTronPlan,
    scope: &TrustedFilmScope,
) -> Result<Value, String> {
    if session.revision != scope.current_revision {
        return Err(reject("NATIVE_SESSION_REVISION_CONFLICT"));
    }
    if session.state.active_sequence.map(|id| id.0) != scope.sequence_id {
        return Err(reject("NATIVE_ACTIVE_SEQUENCE_CHANGED"));
    }
    if let Some(sequence_id) = session.state.active_sequence {
        let Some(sequence) = session.project.sequence(sequence_id) else {
            return Err(reject("SEQUENCE_REMOVED"));
        };
        let rate = sequence.settings.frame_rate;
        if rate.num != scope.fps_num || rate.den != scope.fps_den {
            return Err(reject("NATIVE_SEQUENCE_TIMEBASE_CHANGED"));
        }
    }
    if plan.steps.iter().any(|step| is_file_operation(&step.tool)) && !adapter.has_workspace_root() {
        return Err(reject("NATIVE_WORKSPACE_ROOT_REQUIRED"));
    }
    let preliminary = preflight(plan, scope)?;
    let catalog = semantic_tools();
    let mut receipts = Vec::new();
    for step in &plan.steps {
        let tool = catalog.iter().find(|s| s.name == step.tool)
            .ok_or_else(|| reject("UNSUPPORTED_SEMANTIC_TOOL"))?;
        let (effective_tool, mut params, follow_up) =
            normalize_semantic_step_params(session, &step.tool, &step.params);
        if tool.mutating {
            params["expectedRevision"] = json!(session.revision);
            params["idempotencyKey"] = json!(format!("tron:{}:{}", plan.plan_id, step.id));
        }
        let mut native_result = adapter.execute_on_session(session, &FederatedToolCall {
            call_id: format!("tron:{}:{}", plan.plan_id, step.id),
            tool_name: effective_tool.clone(),
            arguments: params.clone(),
            granted_scopes: scope.granted_scopes.iter().cloned().collect(),
        });
        let mut ok = native_result.get("ok").and_then(Value::as_bool) == Some(true);
        if !ok && effective_tool == "film.timeline.trim" && params.get("mode") == Some(&json!("ripple")) {
            let mut fallback_params = params.clone();
            fallback_params["mode"] = json!("regular");
            fallback_params["expectedRevision"] = json!(session.revision);
            fallback_params["idempotencyKey"] = json!(format!("tron:{}:{}:reg", plan.plan_id, step.id));
            let retry_res = adapter.execute_on_session(session, &FederatedToolCall {
                call_id: format!("tron:{}:{}:reg", plan.plan_id, step.id),
                tool_name: effective_tool.clone(),
                arguments: fallback_params,
                granted_scopes: scope.granted_scopes.iter().cloned().collect(),
            });
            if retry_res.get("ok").and_then(Value::as_bool) == Some(true) {
                native_result = retry_res;
                ok = true;
            }
        }
        receipts.push(native_result);
        if !ok {
            return Ok(json!({
                "status": "PARTIAL_EXECUTION_FAILED",
                "applied": false,
                "completedStepCount": receipts.len().saturating_sub(1),
                "nativeRevision": session.revision,
                "receipts": receipts,
                "preflight": preliminary,
                "needsUserReview": true
            }));
        }
        for (sub_idx, (sub_tool, mut sub_params)) in follow_up.into_iter().enumerate() {
            sub_params["expectedRevision"] = json!(session.revision);
            sub_params["idempotencyKey"] = json!(format!("tron:{}:{}:sub{}", plan.plan_id, step.id, sub_idx));
            let _ = adapter.execute_on_session(session, &FederatedToolCall {
                call_id: format!("tron:{}:{}:sub{}", plan.plan_id, step.id, sub_idx),
                tool_name: sub_tool,
                arguments: sub_params,
                granted_scopes: scope.granted_scopes.iter().cloned().collect(),
            });
        }
    }
    Ok(json!({
        "status": "EXECUTED_NATIVE_RECEIPTS",
        "applied": true,
        "nativeRevision": session.revision,
        "receipts": receipts,
        "preflight": preliminary,
        "visualReviewRequired": true,
        "saveReopenExportNotImplicit": true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> TrustedFilmScope {
        TrustedFilmScope {
            project_id: "film-project-1".to_owned(),
            current_revision: 7,
            sequence_id: Some(42),
            fps_num: 30000,
            fps_den: 1001,
            granted_scopes: ["filmtron:edit".into()].into_iter().collect(),
            allowed_tools: ["film.timeline.add_marker".into(), "film.render.export_media".into()].into_iter().collect(),
            allow_mutations: true,
            approved_external_writes: false,
        }
    }

    fn marker() -> FilmTronPlan {
        FilmTronPlan {
            schema: PLAN_SCHEMA.to_owned(),
            product: "filmtron".to_owned(),
            project_id: "film-project-1".to_owned(),
            plan_id: "plan-001".to_owned(),
            expected_revision: 7,
            sequence_id: Some(42),
            fps_num: 30000,
            fps_den: 1001,
            steps: vec![FilmTronStep {
                id: "shot-001".to_owned(),
                tool: "film.timeline.add_marker".to_owned(),
                params: json!({"positionTick": FrameRate::new(30000, 1001).tick_of(30).0}),
            }],
        }
    }

    #[test]
    fn exactly_frame_aligned_markers_pass_30000_1001() {
        let receipt = preflight(&marker(), &scope());
        assert!(receipt.is_ok(), "{receipt:?}");
        assert_eq!(receipt.ok().map(|v| v["applied"].clone()), Some(json!(false)));
    }

    #[test]
    fn fractional_floating_timecode_and_one_tick_drift_denied() {
        let mut p = marker();
        p.steps[0].params = json!({"positionTick": 1.5});
        assert!(preflight(&p, &scope()).is_err());
        p.steps[0].params = json!({"positionTick": 1});
        assert!(preflight(&p, &scope()).is_err());
        p.steps[0].params = json!({"positionTick": "00:00:01:00"});
        assert!(preflight(&p, &scope()).is_err());
    }

    #[test]
    fn cross_project_stale_revision_wrong_sequence_and_fps_denied() {
        let mut p = marker();
        p.project_id = "other-project".into();
        assert!(preflight(&p, &scope()).is_err());
        p = marker(); p.expected_revision += 1;
        assert!(preflight(&p, &scope()).is_err());
        p = marker(); p.sequence_id = Some(99);
        assert!(preflight(&p, &scope()).is_err());
        p = marker(); p.fps_den = 1;
        assert!(preflight(&p, &scope()).is_err());
    }

    #[test]
    fn forged_grants_shell_and_duplicate_steps_are_denied() {
        let mut p = marker();
        p.steps[0].params = json!({"positionTick": 0, "grantedScopes": ["filmtron:*"]});
        assert!(preflight(&p, &scope()).is_err());
        p = marker(); p.steps[0].tool = "shell.execute".into();
        assert!(preflight(&p, &scope()).is_err());
        p = marker(); p.steps.push(p.steps[0].clone());
        assert!(preflight(&p, &scope()).is_err());
        let mut s = scope(); s.allow_mutations = false;
        assert!(preflight(&marker(), &s).is_err());
    }

    #[test]
    fn export_needs_real_additional_approval() {
        let mut p = marker();
        p.steps[0].tool = "film.render.export_media".into();
        let mut s = scope(); s.granted_scopes.insert("filmtron:render".into());
        assert!(preflight(&p, &s).is_err());
        s.approved_external_writes = true;
        assert!(preflight(&p, &s).is_ok());
    }

    #[test]
    fn execute_checked_normalizes_semantic_params_and_mutates_real_session() {
        let mut session = Session::default();
        session.execute("file.newSequence", json!({
            "name": "Commercial 15s",
            "width": 1080,
            "height": 1920,
            "fps": 30.0
        })).expect("new sequence");
        let bars_res = session.execute("file.newBarsAndTone", json!({"name": "Clip 1"})).expect("bars 1");
        let item1 = bars_res["item"].as_u64().expect("item1 id");
        let bars_res2 = session.execute("file.newBarsAndTone", json!({"name": "Clip 2"})).expect("bars 2");
        let item2 = bars_res2["item"].as_u64().expect("item2 id");

        let seq_id = session.state.active_sequence.map(|id| id.0);
        let rate = FrameRate::new(30, 1);
        let allowed_tools: BTreeSet<String> = semantic_tools().into_iter().map(|t| t.name).collect();
        let exec_scope = TrustedFilmScope {
            project_id: "proj-exec-test".to_owned(),
            current_revision: session.revision,
            sequence_id: seq_id,
            fps_num: 30,
            fps_den: 1,
            granted_scopes: ["filmtron:edit".into(), "filmtron:read".into(), "filmtron:render".into()].into_iter().collect(),
            allowed_tools,
            allow_mutations: true,
            approved_external_writes: false,
        };

        let plan = FilmTronPlan {
            schema: PLAN_SCHEMA.to_owned(),
            product: "filmtron".to_owned(),
            project_id: "proj-exec-test".to_owned(),
            plan_id: "plan-exec-01".to_owned(),
            expected_revision: session.revision,
            sequence_id: seq_id,
            fps_num: 30,
            fps_den: 1,
            steps: vec![
                FilmTronStep {
                    id: "s1_place_1".into(),
                    tool: "film.timeline.place_clip".into(),
                    params: json!({
                        "item": item1.to_string(),
                        "track": 0,
                        "startTick": rate.tick_of(0).0,
                        "inTick": rate.tick_of(0).0,
                        "durationTicks": rate.tick_of(90).0
                    }),
                },
                FilmTronStep {
                    id: "s2_place_2".into(),
                    tool: "film.timeline.place_clip".into(),
                    params: json!({
                        "item": item2,
                        "track": 0,
                        "startTick": rate.tick_of(90).0,
                        "inTick": rate.tick_of(0).0,
                        "durationTicks": rate.tick_of(90).0
                    }),
                },
                FilmTronStep {
                    id: "s3_transition".into(),
                    tool: "film.transitions.apply".into(),
                    params: json!({
                        "effect": "cross_dissolve",
                        "frames": 10,
                        "edge": "out"
                    }),
                },
                FilmTronStep {
                    id: "s4_lumetri".into(),
                    tool: "film.color.set_lumetri".into(),
                    params: json!({
                        "name": "Warm",
                        "contrast": 16,
                        "temperature": 8
                    }),
                },
                FilmTronStep {
                    id: "s5_title".into(),
                    tool: "film.titles.add_text".into(),
                    params: json!({
                        "text": "إعلان سينمائي متكامل",
                        "position": [540, 240],
                        "size": 68,
                        "track": 1,
                        "startTick": rate.tick_of(0).0,
                        "durationTicks": rate.tick_of(90).0,
                        "align": "center",
                        "fillColor": "#FFF8EB",
                        "shadow": true
                    }),
                },
                FilmTronStep {
                    id: "s6_caption".into(),
                    tool: "film.captions.add".into(),
                    params: json!({
                        "track": "C1",
                        "text": "تفاصيل مصممة بدقة عالية",
                        "startTick": rate.tick_of(90).0,
                        "durationTicks": rate.tick_of(90).0
                    }),
                },
                FilmTronStep {
                    id: "s7_audio_gain".into(),
                    tool: "film.audio.set_gain".into(),
                    params: json!({
                        "mode": "set",
                        "db": -5.5
                    }),
                },
                FilmTronStep {
                    id: "s8_marker".into(),
                    tool: "film.timeline.add_marker".into(),
                    params: json!({
                        "positionTick": rate.tick_of(90).0,
                        "name": "Beat Cut",
                        "color": "emerald"
                    }),
                },
            ],
        };

        let mut adapter = OutboundBridgeAdapter::new(None);
        let res = execute_checked(&mut session, &mut adapter, &plan, &exec_scope).expect("execute_checked");
        assert_eq!(res["applied"], true, "Expected applied=true, got: {res}");
        assert_eq!(res["status"], "EXECUTED_NATIVE_RECEIPTS");
    }
}
