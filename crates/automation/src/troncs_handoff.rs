//! Canonical `troncs.handoff.v1` and `troncs.asset-link.v1` provenance validator
//! recognizing all 7 TronCS desktop creative engines (`phototron`, `filmtron`, `aftertron`,
//! `gemimotion`, `figtron`, `lighttron`, `scaletron`) and their 7 `-mobile` companions.

use std::path::Component;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::AutomationError;

pub const TRONCS_HANDOFF_SCHEMA_V1: &str = "troncs.handoff.v1";
pub const TRONCS_ASSET_LINK_SCHEMA_V1: &str = "troncs.asset-link.v1";

/// All 7 canonical TronCS desktop creative suite peers.
pub const TRONCS_DESKTOP_PEERS: [&str; 7] = [
    "phototron",
    "filmtron",
    "aftertron",
    "gemimotion",
    "figtron",
    "lighttron",
    "scaletron",
];

/// All 14 canonical TronCS creative suite peers (7 desktop + 7 `-mobile` companions).
pub const TRONCS_ALL_SUITE_PEERS: [&str; 14] = [
    "phototron",
    "filmtron",
    "aftertron",
    "gemimotion",
    "figtron",
    "lighttron",
    "scaletron",
    "phototron-mobile",
    "filmtron-mobile",
    "aftertron-mobile",
    "gemimotion-mobile",
    "figtron-mobile",
    "lighttron-mobile",
    "scaletron-mobile",
];

/// Returns `true` when `peer_id` is a recognized TronCS desktop or mobile peer.
pub fn is_recognized_troncs_peer(peer_id: &str) -> bool {
    let normalized = peer_id.trim();
    TRONCS_ALL_SUITE_PEERS.contains(&normalized)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LossSeverity {
    Lossless,
    Convertible,
    Baked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LossReportItem {
    pub feature: String,
    pub classification: LossSeverity,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffEndpoint {
    pub product: String,
    #[serde(default)]
    pub document_id: Option<String>,
    #[serde(default)]
    pub revision: Option<u64>,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffAssetRef {
    pub relative_path: String,
    pub sha256: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub color_space: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TronCsHandoffManifest {
    pub schema: String,
    pub handoff_id: String,
    pub source: HandoffEndpoint,
    pub target: HandoffEndpoint,
    pub assets: Vec<HandoffAssetRef>,
    #[serde(default)]
    pub loss_report: Vec<LossReportItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TronCsAssetLink {
    pub schema: String,
    pub link_id: String,
    pub producer_product: String,
    pub consumer_product: String,
    pub producer_document_id: String,
    pub producer_revision: u64,
    pub relative_path: String,
    pub sha256: String,
}

fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_safe_workspace_relative_path(rel: &str) -> bool {
    if rel.trim().is_empty() || rel.contains('\0') || rel.contains('\\') {
        return false;
    }
    let path = Path::new(rel);
    if path.is_absolute() {
        return false;
    }
    let mut saw_normal = false;
    for comp in path.components() {
        match comp {
            Component::Normal(_) => saw_normal = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    saw_normal
}

/// Validates a `troncs.handoff.v1` manifest for provenance, SHA-256 digests, relative paths,
/// and membership in the 14 canonical TronCS desktop/mobile peers.
pub fn validate_handoff_manifest(manifest: &TronCsHandoffManifest) -> Result<(), AutomationError> {
    if manifest.schema != TRONCS_HANDOFF_SCHEMA_V1 {
        return Err(AutomationError::BadRequest(format!(
            "unsupported handoff schema '{}', expected '{TRONCS_HANDOFF_SCHEMA_V1}'",
            manifest.schema
        )));
    }
    if manifest.handoff_id.trim().is_empty() {
        return Err(AutomationError::BadRequest(
            "handoffId must not be empty".into(),
        ));
    }
    if !is_recognized_troncs_peer(&manifest.source.product) {
        return Err(AutomationError::BadRequest(format!(
            "unrecognized TronCS source product '{}'",
            manifest.source.product
        )));
    }
    if !is_recognized_troncs_peer(&manifest.target.product) {
        return Err(AutomationError::BadRequest(format!(
            "unrecognized TronCS target product '{}'",
            manifest.target.product
        )));
    }
    if manifest.assets.is_empty() {
        return Err(AutomationError::BadRequest(
            "handoff manifest must declare at least one asset".into(),
        ));
    }
    for asset in &manifest.assets {
        if !is_safe_workspace_relative_path(&asset.relative_path) {
            return Err(AutomationError::BadRequest(format!(
                "unsafe or non-relative handoff asset path '{}'",
                asset.relative_path
            )));
        }
        if !is_valid_sha256_hex(&asset.sha256) {
            return Err(AutomationError::BadRequest(format!(
                "invalid SHA-256 hex digest on asset '{}'",
                asset.relative_path
            )));
        }
    }
    Ok(())
}

/// Validates a `troncs.asset-link.v1` sidecar link across all 14 TronCS desktop/mobile peers.
pub fn validate_asset_link(link: &TronCsAssetLink) -> Result<(), AutomationError> {
    if link.schema != TRONCS_ASSET_LINK_SCHEMA_V1 {
        return Err(AutomationError::BadRequest(format!(
            "unsupported asset-link schema '{}', expected '{TRONCS_ASSET_LINK_SCHEMA_V1}'",
            link.schema
        )));
    }
    if link.link_id.trim().is_empty() || link.producer_document_id.trim().is_empty() {
        return Err(AutomationError::BadRequest(
            "linkId and producerDocumentId must not be empty".into(),
        ));
    }
    if !is_recognized_troncs_peer(&link.producer_product) {
        return Err(AutomationError::BadRequest(format!(
            "unrecognized TronCS producer product '{}'",
            link.producer_product
        )));
    }
    if !is_recognized_troncs_peer(&link.consumer_product) {
        return Err(AutomationError::BadRequest(format!(
            "unrecognized TronCS consumer product '{}'",
            link.consumer_product
        )));
    }
    if !is_safe_workspace_relative_path(&link.relative_path) {
        return Err(AutomationError::BadRequest(format!(
            "unsafe relative path '{}'",
            link.relative_path
        )));
    }
    if !is_valid_sha256_hex(&link.sha256) {
        return Err(AutomationError::BadRequest(
            "invalid SHA-256 hex digest on asset link".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemi_tokens::{
        NEUTRAL_MEDIA_VIEWPORT, NEUTRAL_SCOPE_PLOT, PRESET_IDS, preset,
    };

    #[test]
    fn canonical_20_gemi_tokens_presets_and_neutral_viewports_verified() -> Result<(), String> {
        if PRESET_IDS.len() != 20 {
            return Err(format!("expected 20 presets, got {}", PRESET_IDS.len()));
        }
        if NEUTRAL_MEDIA_VIEWPORT.rgba() != [24, 24, 24, 255] || !NEUTRAL_MEDIA_VIEWPORT.is_neutral() {
            return Err("NEUTRAL_MEDIA_VIEWPORT must equal RGB(24,24,24)".into());
        }
        if NEUTRAL_SCOPE_PLOT.rgba() != [18, 18, 18, 255] || !NEUTRAL_SCOPE_PLOT.is_neutral() {
            return Err("NEUTRAL_SCOPE_PLOT must equal RGB(18,18,18)".into());
        }
        for id in PRESET_IDS {
            let dark = preset(id, true).ok_or_else(|| format!("missing dark preset {id}"))?;
            let light = preset(id, false).ok_or_else(|| format!("missing light preset {id}"))?;
            if !dark.dark || light.dark {
                return Err(format!("dark/light flag mismatch on {id}"));
            }
        }
        if preset("unknownPreset", true).is_some() {
            return Err("unknown preset should return None".into());
        }
        Ok(())
    }

    #[test]
    fn validates_all_7_desktop_and_7_mobile_troncs_suite_peers() -> Result<(), String> {
        let digest = "b".repeat(64);
        for peer in TRONCS_ALL_SUITE_PEERS {
            let manifest = TronCsHandoffManifest {
                schema: TRONCS_HANDOFF_SCHEMA_V1.into(),
                handoff_id: format!("handoff-{peer}"),
                source: HandoffEndpoint {
                    product: peer.into(),
                    document_id: Some("seq-001".into()),
                    revision: Some(9),
                    format: Some("mov".into()),
                },
                target: HandoffEndpoint {
                    product: "filmtron".into(),
                    document_id: None,
                    revision: None,
                    format: Some("fcproj".into()),
                },
                assets: vec![HandoffAssetRef {
                    relative_path: "renders/plate_4k.png".into(),
                    sha256: digest.clone(),
                    mime_type: Some("image/png".into()),
                    color_space: Some("Rec.709".into()),
                    width: Some(3840),
                    height: Some(2160),
                }],
                loss_report: vec![LossReportItem {
                    feature: "rational_ticks_254016000000".into(),
                    classification: LossSeverity::Lossless,
                    detail: "Exact frame-accurate cut points preserved".into(),
                }],
            };
            validate_handoff_manifest(&manifest).map_err(|e| e.to_string())?;

            let link = TronCsAssetLink {
                schema: TRONCS_ASSET_LINK_SCHEMA_V1.into(),
                link_id: format!("link-{peer}"),
                producer_product: peer.into(),
                consumer_product: "filmtron-mobile".into(),
                producer_document_id: "seq-001".into(),
                producer_revision: 9,
                relative_path: "renders/plate_4k.png".into(),
                sha256: digest.clone(),
            };
            validate_asset_link(&link).map_err(|e| e.to_string())?;
        }

        let bad_peer = TronCsAssetLink {
            schema: TRONCS_ASSET_LINK_SCHEMA_V1.into(),
            link_id: "bad".into(),
            producer_product: "unknown-app".into(),
            consumer_product: "filmtron".into(),
            producer_document_id: "seq-001".into(),
            producer_revision: 1,
            relative_path: "renders/a.png".into(),
            sha256: digest.clone(),
        };
        if validate_asset_link(&bad_peer).is_ok() {
            return Err("expected unrecognized peer to be rejected".into());
        }

        let traversal = TronCsAssetLink {
            producer_product: "gemimotion".into(),
            relative_path: "../escape.png".into(),
            ..bad_peer
        };
        if validate_asset_link(&traversal).is_ok() {
            return Err("expected parent-dir path traversal to be rejected".into());
        }
        Ok(())
    }
}
