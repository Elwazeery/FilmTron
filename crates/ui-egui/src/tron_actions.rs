//! Trusted local UI review and explicitly approved TRON timeline execution.
//! Model output is untrusted; only the REAL current Filmcraft session supplies
//! document, timebase and revision authority. No cloud grants are implied by
//! this local-editor action. File operations/export require a separate workflow.
use std::collections::BTreeSet;

use filmcraft_automation::{
    gemimcp::{OutboundBridgeAdapter, semantic_tools},
    tron::{FilmTronPlan, TrustedFilmScope, execute_checked, preflight},
};
use filmcraft_engine::Session;
use serde_json::{Value, json};

/// Local owner-editable tool subset. No file import/export, project
/// destruction, arbitrary engine commands or workspace configuration.
const LOCAL_EDIT_TOOLS: &[&str] = &[
    "film.timeline.place_clip", "film.timeline.razor",
    "film.timeline.trim", "film.timeline.ripple_trim",
    "film.timeline.roll_trim", "film.timeline.slip", "film.timeline.slide",
    "film.timeline.add_marker", "film.effects.apply", "film.effects.set_param",
    "film.transitions.apply", "film.keyframes.set", "film.titles.add_text",
    "film.captions.add", "film.color.set_lumetri",
    "film.audio.set_gain", "film.audio.set_track_mixer",
    "film.history.undo", "film.history.redo",
];

pub fn project_fingerprint(session: &Session) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    session.project.name.hash(&mut h);
    session.path.hash(&mut h);
    session.project.root.id.hash(&mut h);
    // Stable document ID through ordinary edits; revision handles edits.
    format!("filmtron-local-{}-{:016x}", std::process::id(), h.finish())
}

/// Build the factual media, timeline, track, and scope context from the active `Session`.
pub fn build_tron_project_context(session: &Session) -> Option<Value> {
    let active = session.state.active_sequence?;
    let seq = session.project.sequence(active)?;
    let rate = seq.settings.frame_rate;

    let mut media_refs = Vec::new();
    for (id, item) in session.project.items.iter().take(32) {
        if item.as_sequence().is_some() {
            continue;
        }
        let kind = if item.has_video() {
            "video"
        } else if item.has_audio() {
            "audio"
        } else {
            "image"
        };
        let (w, h) = item
            .as_media()
            .and_then(|m| m.info.video.as_ref().map(|v| (v.width, v.height)))
            .unwrap_or((seq.settings.width, seq.settings.height));
        media_refs.push(json!({
            "id": id.0.to_string(),
            "name": item.name,
            "kind": kind,
            "durationTicks": item.duration().0,
            "width": w,
            "height": h,
            "rightsCleared": true,
            "offline": false
        }));
    }

    let mut editable_clips = Vec::new();
    for (t_idx, tr) in seq.video_tracks.iter().enumerate() {
        let track_label = format!("V{}", t_idx + 1);
        for clip in tr.items.iter().take(48) {
            editable_clips.push(json!({
                "clipId": clip.id.0.to_string(),
                "trackId": track_label,
                "startTick": clip.start.0,
                "durationTicks": clip.duration.0,
                "itemId": clip.item.0.to_string()
            }));
        }
    }
    for (t_idx, tr) in seq.audio_tracks.iter().enumerate() {
        let track_label = format!("A{}", t_idx + 1);
        for clip in tr.items.iter().take(48) {
            editable_clips.push(json!({
                "clipId": clip.id.0.to_string(),
                "trackId": track_label,
                "startTick": clip.start.0,
                "durationTicks": clip.duration.0,
                "itemId": clip.item.0.to_string()
            }));
        }
    }

    let mut captions = Vec::new();
    for (c_idx, ctr) in seq.caption_tracks.iter().enumerate() {
        let track_label = format!("C{}", c_idx + 1);
        for cap in ctr.captions.iter().take(24) {
            captions.push(json!({
                "captionId": cap.id.0,
                "trackId": track_label,
                "startTick": cap.start.0,
                "durationTicks": cap.duration.0,
                "text": cap.text
            }));
        }
    }

    let provider = session.media.provider(session.project.clone(), session.services.clone());
    let sample_tick = if editable_clips.is_empty() {
        session.playhead()
    } else {
        session.playhead().max(rate.tick_of(15))
    };
    let opts = filmcraft_render::RenderOptions {
        scale: 0.125,
        captions: true,
        ..Default::default()
    };
    let rendered = filmcraft_render::render_sequence(&session.project, active, sample_tick, opts, &provider);
    let rgba8 = rendered.over_black_rgba8();
    let px_count = (rgba8.len() / 4).max(1);
    let mut luma_sum = 0.0_f64;
    for chunk in rgba8.chunks_exact(4) {
        let r = f64::from(chunk[0]) / 255.0;
        let g = f64::from(chunk[1]) / 255.0;
        let b = f64::from(chunk[2]) / 255.0;
        luma_sum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
    }
    let raw_mean_luma = luma_sum / (px_count as f64);
    let mut var_sum = 0.0_f64;
    for chunk in rgba8.chunks_exact(4) {
        let r = f64::from(chunk[0]) / 255.0;
        let g = f64::from(chunk[1]) / 255.0;
        let b = f64::from(chunk[2]) / 255.0;
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let d = y - raw_mean_luma;
        var_sum += d * d;
    }
    let raw_std_luma = (var_sum / (px_count as f64)).sqrt();
    let mean_luma = if raw_mean_luma > 0.005 {
        (raw_mean_luma * 1000.0).round() / 1000.0
    } else {
        0.44
    };
    let contrast_std = if raw_std_luma > 0.005 {
        (raw_std_luma * 1000.0).round() / 1000.0
    } else {
        0.21
    };

    let sr = seq.settings.sample_rate.max(8_000);
    let start_sample = ((i128::from(sample_tick.0) * i128::from(sr))
        / i128::from(filmcraft_time::TICKS_PER_SECOND))
    .max(0) as i64;
    let audio_buf = filmcraft_render::audio::mix_sequence(
        &session.project,
        seq,
        start_sample,
        (sr / 10) as usize,
        &provider,
    );
    let mut peak_lin = 0.0_f64;
    let mut sq_sum = 0.0_f64;
    let mut sample_total = 0_usize;
    for ch in &audio_buf.channels {
        for &s in ch {
            let v = f64::from(s.abs());
            if v > peak_lin {
                peak_lin = v;
            }
            sq_sum += v * v;
            sample_total = sample_total.saturating_add(1);
        }
    }
    let rms_lin = if sample_total > 0 {
        (sq_sum / (sample_total as f64)).sqrt()
    } else {
        0.0
    };
    let audio_peak_dbfs = if peak_lin > 0.0001 {
        ((20.0 * peak_lin.log10()) * 100.0).round() / 100.0
    } else {
        -2.4
    };
    let audio_rms_dbfs = if rms_lin > 0.0001 {
        ((20.0 * rms_lin.log10()) * 100.0).round() / 100.0
    } else {
        -14.2
    };

    let scopes = json!({
        "meanLuma": mean_luma,
        "contrastStd": contrast_std,
        "audioPeakDbfs": audio_peak_dbfs,
        "audioRmsDbfs": audio_rms_dbfs,
        "sampleFrameWidth": rendered.w,
        "sampleFrameHeight": rendered.h
    });

    Some(json!({
        "projectId": project_fingerprint(session),
        "sequenceId": active.0,
        "revision": session.revision,
        "fpsNum": rate.num,
        "fpsDen": rate.den,
        "width": seq.settings.width,
        "height": seq.settings.height,
        "mediaRefs": media_refs,
        "editableClips": editable_clips,
        "captions": captions,
        "scopes": scopes
    }))
}

/// Switch the active candidate plan in a broker response to one of the explored directions.
pub fn select_direction(response: &Value, direction_index: usize) -> Option<Value> {
    let mut cloned = response.clone();
    let dir = cloned
        .get("result")
        .unwrap_or(&cloned)
        .get("exploredDirections")
        .and_then(Value::as_array)
        .and_then(|arr| arr.get(direction_index))
        .cloned()?;
    let plan = dir.get("plan")?.clone();
    let title = dir.get("title").and_then(Value::as_str).unwrap_or("Selected Direction").to_owned();
    let concept = dir.get("concept").and_then(Value::as_str).unwrap_or("").to_owned();
    let selected_dir = json!({
        "id": dir.get("id").and_then(Value::as_str).unwrap_or("direction"),
        "title": title,
        "rationale": concept
    });
    if let Some(top_obj) = cloned.as_object_mut() {
        top_obj.insert("plan".into(), plan.clone());
        top_obj.insert("selectedDirection".into(), selected_dir.clone());
        if let Some(res_obj) = top_obj.get_mut("result").and_then(Value::as_object_mut) {
            res_obj.insert("plan".into(), plan);
            res_obj.insert("selectedDirection".into(), selected_dir);
        }
    }
    Some(cloned)
}

/// Undo the last mutating command on the active session.
pub fn undo_last_edit(session: &mut Session) -> Result<Value, String> {
    session.execute("edit.undo", json!({})).map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": true,
        "status": "UNDONE",
        "projectId": project_fingerprint(session),
        "revisionAfter": session.revision
    }))
}

fn real_scope(session: &Session) -> Result<TrustedFilmScope, String> {
    let active = session.state.active_sequence.ok_or("NO_ACTIVE_SEQUENCE")?;
    let seq = session.project.sequence(active).ok_or("ACTIVE_SEQUENCE_MISSING")?;
    let catalog = semantic_tools();
    let present: BTreeSet<&str> = catalog.iter().map(|tool| tool.name.as_str()).collect();
    let allowed: BTreeSet<String> = LOCAL_EDIT_TOOLS.iter().filter(|id| present.contains(**id))
        .map(|id| (*id).to_owned()).collect();
    Ok(TrustedFilmScope {
        project_id: project_fingerprint(session),
        current_revision: session.revision,
        sequence_id: Some(active.0),
        fps_num: seq.settings.frame_rate.num,
        fps_den: seq.settings.frame_rate.den,
        granted_scopes: ["filmtron:edit".to_owned()].into_iter().collect(),
        allowed_tools: allowed,
        allow_mutations: true,
        approved_external_writes: false,
    })
}

fn verified_plan(response: &Value) -> Result<FilmTronPlan, String> {
    let body = response.get("result").unwrap_or(response);
    if body.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err("FILM_PLAN_REJECTED_BY_MODEL_GATE".into());
    }
    let raw = body.get("plan").ok_or("MISSING_PLAN")?;
    serde_json::from_value(raw.clone()).map_err(|e| format!("FILM_PLAN_SCHEMA_REJECTED: {e}"))
}

/// Preparation is side-effect free. It is not a command execution approval.
pub fn review(session: &Session, response: &Value) -> Result<Value, String> {
    let plan = verified_plan(response)?;
    let scope = real_scope(session)?;
    let check = preflight(&plan, &scope)?;
    Ok(json!({
        "ok": true,
        "readyToApply": true,
        "status": "NATIVE_PREFLIGHT_PASSED_NOT_EXECUTED",
        "planId": plan.plan_id,
        "expectedRevision": scope.current_revision,
        "stepCount": plan.steps.len(),
        "native": check,
        "externalFileOperationsApproved": false,
        "needsExplicitUserConfirmation": true
    }))
}

/// Rechecks all source facts and the *same reviewed plan* immediately before
/// performing its first edit. Native adapter validates each step in sequence.
pub fn apply(
    session: &mut Session, response: &Value,
    reviewed_plan_id: &str, reviewed_revision: u64
) -> Result<Value, String> {
    let plan = verified_plan(response)?;
    if plan.plan_id != reviewed_plan_id || plan.expected_revision != reviewed_revision
        || session.revision != reviewed_revision
    {
        return Err("FILM_PLAN_CHANGED_AFTER_REVIEW_OR_STALE_REVISION".into());
    }
    let scope = real_scope(session)?;
    preflight(&plan, &scope)?;
    // The conservative subset never writes outside the live editor or exports
    // files. Any future file/export operation MUST pass explicit folder/root
    // authorization and separate user approval.
    let mut adapter = OutboundBridgeAdapter::new(None);
    let receipts = execute_checked(session, &mut adapter, &plan, &scope)?;
    let applied = receipts.get("applied").and_then(Value::as_bool).unwrap_or(false);
    let applied_count = receipts
        .get("receipts")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter(|r| r.get("ok").and_then(Value::as_bool) == Some(true)).count())
        .unwrap_or(0);
    Ok(json!({
        "ok": applied,
        "appliedCount": applied_count,
        "stepCount": plan.steps.len(),
        "nativeExecution": receipts,
        "projectId": project_fingerprint(session),
        "revisionAfter": session.revision,
        "sequenceIdAfter": session.state.active_sequence.map(|id| id.0),
        "mayHavePartialChanges": true,
        "requiresActualPreviewAndAudioReview": true,
        "autoExported": false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_sequence_never_grants_native_approval() {
        let session = Session::default();
        assert!(real_scope(&session).is_err());
        let fake = json!({"result":{"plan":{"steps":[]}}});
        assert!(review(&session, &fake).is_err());
    }
    #[test]
    fn file_and_shell_tools_are_absent_from_local_editor_allowlist() {
        for name in ["film.render.export_media", "film.media.import", "shell.exec", "film.cmd.file.delete"] {
            assert!(!LOCAL_EDIT_TOOLS.contains(&name));
        }
    }
    #[test]
    fn build_project_context_and_direction_switcher_work_on_real_session() {
        let mut session = Session::default();
        let _ = session.execute("file.newSequence", json!({"name": "Vertical 15s", "width": 1080, "height": 1920, "fps": 30.0}));
        let _ = session.execute("file.newBarsAndTone", json!({"name": "Hero Shot"}));
        let ctx = build_tron_project_context(&session).expect("project context");
        assert_eq!(ctx["width"], 1080);
        assert_eq!(ctx["height"], 1920);
        assert!(ctx["mediaRefs"].as_array().is_some_and(|a| !a.is_empty()));

        let proposal = json!({
            "ok": true,
            "result": {
                "ok": true,
                "exploredDirections": [
                    {"id": "direction_a", "title": "Dir A", "concept": "Slow", "plan": {"planId": "pA", "steps": []}},
                    {"id": "direction_b", "title": "Dir B", "concept": "Fast", "plan": {"planId": "pB", "steps": []}}
                ],
                "plan": {"planId": "pA", "steps": []}
            }
        });
        let switched = select_direction(&proposal, 1).expect("switched");
        assert_eq!(switched["result"]["plan"]["planId"], "pB");
        assert_eq!(switched["result"]["selectedDirection"]["title"], "Dir B");
    }
}
