//! FilmTron TRON creative plan gate and canonical native executor.
//!
//! This module does not load a model or create an MCP server. All generated
//! plans are untrusted. An owner-authorized caller constructs TrustedFilmScope
//! from the REAL active editor session, current Gemi Access grants and user
//! approval; the model cannot supply this scope.
use std::collections::BTreeSet;

use filmcraft_engine::Session;
use filmcraft_time::{FrameRate, Tick, TICKS_PER_SECOND};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
        let mut params = step.params.clone();
        if tool.mutating {
            params["expectedRevision"] = json!(session.revision);
            params["idempotencyKey"] = json!(format!("tron:{}:{}", plan.plan_id, step.id));
        }
        let native_result = adapter.execute_on_session(session, &FederatedToolCall {
            call_id: format!("tron:{}:{}", plan.plan_id, step.id),
            tool_name: step.tool.clone(),
            arguments: params,
            granted_scopes: scope.granted_scopes.iter().cloned().collect(),
        });
        let ok = native_result.get("ok").and_then(Value::as_bool) == Some(true);
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
}
