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
    Ok(json!({
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
}
