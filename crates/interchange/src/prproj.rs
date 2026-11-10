//! Clean-room Premiere Pro project (`.prproj`) reader and writer (`<PremiereData>` XML, plain or
//! gzip-compressed).
//!
//! Premiere Pro's `.prproj` container is a gzip stream (`1f 8b`) wrapping an UTF-8 XML document
//! rooted at `<PremiereData>` whose native tick rate is `254,016,000,000` ticks per second — the
//! exact same rational tick base as [`filmcraft_time::Tick`].
//!
//! Supported on import & export:
//! - Gzip-compressed (`.prproj` magic `0x1f 0x8b`) and plain XML `<PremiereData>` documents, with
//!   a strict 64 MiB decompression cap to prevent zip-bomb allocation attacks;
//! - Bins (`<BinProjectItem>` / `<Bin>`), media clips (`<Media>` / `<ClipProjectItem>`), sequences
//!   (`<Sequence>`), video and audio tracks (`<VideoTrack>` / `<AudioTrack>`), clip items
//!   (`<VideoClipTrackItem>` / `<AudioClipTrackItem>` / `<ClipTrackItem>`), transitions
//!   (`<TransitionTrackItem>`), sequence and clip markers (`<Marker>`), and intrinsic/standard
//!   effects (Motion, Opacity, Lumetri Color, Volume);
//! - Any unrecognized or proprietary third-party effect/node is recorded in [`Report`] without
//!   failing the import.

use std::collections::HashMap;
use std::io::{Read, Write};

use filmcraft_project::{
    BinEntry, BinId, EffectInstance, ItemId, ItemKind, Label, Marker, MarkerId, MarkerKind, Param,
    ParamValue, Project, Sequence, TrackItem, TrackKind, Transition, TransitionAlign, TransitionId,
    find_effect,
};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use roxmltree::Node;

use crate::common::{
    Builder, MediaSpec, file_name, item_path, kind_from_ext, resolve_path, set_param, settings_for,
    transition_effect, transition_name,
};
use crate::xml::{
    XmlWriter, bool_str, child, child_bool, child_f64, child_i64, child_text, elements, num,
};
use crate::{Error, ExportOptions, Format, ImportOptions, Imported, Report, Result};

/// Maximum decompressed XML size (64 MiB) to guard against hostile gzip streams.
pub const MAX_DECOMPRESSED_PRPROJ_BYTES: u64 = 64 * 1024 * 1024;

/// Returns `true` if `bytes` is a `.prproj` document (either plain `<PremiereData>` XML or a gzip
/// stream whose header/content starts with `<PremiereData` or has `.prproj` extension).
pub fn sniff(bytes: &[u8], extension: Option<&str>) -> bool {
    let is_prproj_ext = extension
        .map(|e| e.trim_start_matches('.').eq_ignore_ascii_case("prproj"))
        .unwrap_or(false);

    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut decoder = GzDecoder::new(bytes).take(4096);
        let mut head = Vec::with_capacity(512);
        if decoder.read_to_end(&mut head).is_ok() {
            let text = String::from_utf8_lossy(&head);
            if text.contains("<PremiereData") {
                return true;
            }
        }
        return is_prproj_ext;
    }

    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    trimmed.starts_with('<') && trimmed.contains("<PremiereData")
}

/// Decompress a gzipped `.prproj` byte slice (or decode plain UTF-8 XML) with a strict 64 MiB cap.
pub fn decode_prproj_xml(bytes: &[u8]) -> Result<String> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut decoder = GzDecoder::new(bytes).take(MAX_DECOMPRESSED_PRPROJ_BYTES + 1);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| Error::parse(Format::Prproj, format!("invalid gzip stream: {e}")))?;
        if out.len() as u64 > MAX_DECOMPRESSED_PRPROJ_BYTES {
            return Err(Error::parse(
                Format::Prproj,
                "decompressed .prproj exceeds 64 MiB safety limit",
            ));
        }
        let s = String::from_utf8(out)
            .map_err(|e| Error::parse(Format::Prproj, format!("invalid UTF-8 in .prproj: {e}")))?;
        Ok(s)
    } else {
        let b = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        if b.len() as u64 > MAX_DECOMPRESSED_PRPROJ_BYTES {
            return Err(Error::parse(
                Format::Prproj,
                ".prproj XML exceeds 64 MiB safety limit",
            ));
        }
        std::str::from_utf8(b)
            .map(str::to_owned)
            .map_err(|e| Error::parse(Format::Prproj, format!("invalid UTF-8 in .prproj: {e}")))
    }
}

/// Import a `.prproj` document from raw bytes (gzipped or plain XML).
pub fn import_bytes(
    bytes: &[u8],
    opts: &ImportOptions,
    report: &mut Report,
) -> Result<Imported> {
    let xml = decode_prproj_xml(bytes)?;
    import(&xml, opts, report)
}

fn rate_from_ticks_per_frame(ticks_per_frame: i64) -> FrameRate {
    if ticks_per_frame <= 0 {
        return FrameRate::FPS_24;
    }
    for rate in FrameRate::COMMON {
        if (rate.frame_duration().0 - ticks_per_frame).abs() <= 1000 {
            return rate;
        }
    }
    let approx_fps = (TICKS_PER_SECOND as f64 / ticks_per_frame as f64).round() as i64;
    FrameRate::new(approx_fps.clamp(1, 240), 1)
}

fn read_marker_node(m: Node<'_, '_>, id: u64) -> Option<Marker> {
    let start = child_i64(m, "Start")
        .or_else(|| m.attribute("Start").and_then(|s| s.parse::<i64>().ok()))
        .map(Tick)?;
    let duration = child_i64(m, "Duration")
        .or_else(|| m.attribute("Duration").and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(0)
        .max(0);
    let name = child_text(m, "Name")
        .or_else(|| m.attribute("Name"))
        .unwrap_or("")
        .to_owned();
    let comment = child_text(m, "Comment")
        .or_else(|| child_text(m, "Comments"))
        .or_else(|| m.attribute("Comment"))
        .unwrap_or("")
        .to_owned();
    let kind = match child_text(m, "Type").or_else(|| m.attribute("Type")) {
        Some(t) if t.eq_ignore_ascii_case("chapter") => MarkerKind::Chapter,
        _ => MarkerKind::Comment,
    };
    Some(Marker {
        id: MarkerId(id),
        start,
        duration: Tick(duration),
        name,
        comment,
        kind,
        color: Label::Green,
    })
}

fn apply_prproj_effect_node(ti: &mut TrackItem, comp: Node<'_, '_>, report: &mut Report) {
    let match_name = comp
        .attribute("MatchName")
        .or_else(|| child_text(comp, "MatchName"))
        .or_else(|| comp.attribute("Name"))
        .or_else(|| child_text(comp, "Name"))
        .unwrap_or("UnknownEffect");
    let lower = match_name.to_ascii_lowercase();

    if lower.contains("motion") || lower == "adbe motion" {
        if let Some(scale) = child_f64(comp, "Scale") {
            set_param(ti, "motion", "scale", Param::new(ParamValue::Float(scale)));
        }
        if let Some(rot) = child_f64(comp, "Rotation") {
            set_param(ti, "motion", "rotation", Param::new(ParamValue::Float(rot)));
        }
        return;
    }

    if lower.contains("opacity") || lower == "adbe opacity" {
        if let Some(op) = child_f64(comp, "Opacity").or_else(|| child_f64(comp, "Value")) {
            set_param(
                ti,
                "opacity",
                "opacity",
                Param::new(ParamValue::Float(op.clamp(0.0, 100.0))),
            );
        }
        return;
    }

    if lower.contains("lumetri") || lower.contains("color") {
        for (xml_key, param_id) in [
            ("Exposure", "exposure"),
            ("Contrast", "contrast"),
            ("Highlights", "highlights"),
            ("Shadows", "shadows"),
            ("Whites", "whites"),
            ("Blacks", "blacks"),
            ("Temperature", "temperature"),
            ("Tint", "tint"),
            ("Saturation", "saturation"),
            ("Vibrance", "vibrance"),
        ] {
            if let Some(val) = child_f64(comp, xml_key) {
                set_param(
                    ti,
                    "lumetri",
                    param_id,
                    Param::new(ParamValue::Float(val)),
                );
            }
        }
        return;
    }

    if lower.contains("volume") || lower.contains("gain") || lower == "adbe audio volume" {
        if let Some(db) = child_f64(comp, "LevelDb").or_else(|| child_f64(comp, "GainDb")) {
            ti.gain_db = db.clamp(-96.0, 24.0);
        }
        return;
    }

    if let Some(def) = find_effect(&lower) {
        let mut inst: EffectInstance = def.instance();
        for p in elements(comp) {
            if let Some(v) = p.text().and_then(|t| t.trim().parse::<f64>().ok()) {
                inst.params.insert(
                    p.tag_name().name().to_ascii_lowercase(),
                    Param::new(ParamValue::Float(v)),
                );
            }
        }
        ti.effects.push(inst);
    } else {
        report.warn(format!(
            "unsupported Premiere effect \"{match_name}\" preserved in compatibility report"
        ));
    }
}

/// Import a decoded `<PremiereData>` XML string into an [`Imported`] project fragment.
pub fn import(text_in: &str, opts: &ImportOptions, report: &mut Report) -> Result<Imported> {
    let doc = crate::xml::parse(text_in).map_err(|e| Error::parse(Format::Prproj, e))?;
    let root = doc.root_element();
    if root.tag_name().name() != "PremiereData" {
        return Err(Error::parse(
            Format::Prproj,
            "root element is not <PremiereData>",
        ));
    }

    let proj_name = opts
        .name
        .as_deref()
        .or_else(|| root.attribute("Name"))
        .or_else(|| {
            root.descendants()
                .find(|n| n.has_tag_name("Project"))
                .and_then(|p| child_text(p, "Name").or_else(|| p.attribute("Name")))
        })
        .unwrap_or("Imported Premiere Project");

    let mut b = Builder::new(proj_name);
    let mut bin_map: HashMap<String, BinId> = HashMap::new();
    let mut media_by_id: HashMap<String, ItemId> = HashMap::new();

    // 1. Collect bins (`<Bin>` or `<BinProjectItem>`)
    for n in root
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "Bin" | "BinProjectItem"))
    {
        let name = child_text(n, "Name")
            .or_else(|| n.attribute("Name"))
            .unwrap_or("Bin");
        let parent_bin = n
            .attribute("ParentBin")
            .and_then(|pid| bin_map.get(pid).copied());
        let bid = b.bin(name, parent_bin);
        if let Some(id_attr) = n.attribute("ObjectID").or_else(|| n.attribute("id")) {
            bin_map.insert(id_attr.to_owned(), bid);
        }
    }

    // 2. Collect media items (`<Media>` or `<ClipProjectItem>`)
    for n in root
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "Media" | "ClipProjectItem"))
    {
        let raw_path = child_text(n, "FilePath")
            .or_else(|| child_text(n, "ActualMediaFilePath"))
            .or_else(|| n.attribute("FilePath"))
            .unwrap_or("");
        if raw_path.is_empty() {
            continue;
        }
        let resolved = resolve_path(raw_path, opts.base_dir.as_deref());
        let title = child_text(n, "Title")
            .or_else(|| child_text(n, "Name"))
            .or_else(|| n.attribute("Name"))
            .unwrap_or_else(|| file_name(&resolved));
        let duration_ticks = child_i64(n, "Duration")
            .or_else(|| n.attribute("Duration").and_then(|d| d.parse::<i64>().ok()))
            .unwrap_or(0)
            .max(0);
        let width = child_i64(n, "Width").unwrap_or(1920).clamp(1, 16384) as u32;
        let height = child_i64(n, "Height").unwrap_or(1080).clamp(1, 16384) as u32;
        let frame_rate = child_i64(n, "FrameRate")
            .map(rate_from_ticks_per_frame)
            .unwrap_or(FrameRate::FPS_24);
        let sample_rate = child_i64(n, "AudioSampleRate")
            .unwrap_or(48_000)
            .clamp(8_000, 192_000) as u32;
        let channels = child_i64(n, "AudioChannels").unwrap_or(2).clamp(1, 32) as u32;
        let kind = kind_from_ext(&resolved);
        let spec = MediaSpec {
            duration: Some(Tick(duration_ticks)),
            video: (kind != filmcraft_media::MediaKind::AudioOnly)
                .then_some((width, height, frame_rate)),
            audio: (kind != filmcraft_media::MediaKind::Still)
                .then_some((sample_rate, channels)),
            start_tc: child_i64(n, "StartTimecode"),
            kind: Some(kind),
        };
        let parent_bin = n
            .attribute("Bin")
            .or_else(|| child_text(n, "Bin"))
            .and_then(|bid| bin_map.get(bid).copied());
        let item_id = b.file_media(&resolved, title, &resolved, &spec, parent_bin);
        if let Some(obj_id) = n
            .attribute("ObjectID")
            .or_else(|| n.attribute("ObjectUID"))
            .or_else(|| n.attribute("id"))
        {
            media_by_id.insert(obj_id.to_owned(), item_id);
        }
    }

    // 3. Collect sequences (`<Sequence>`)
    for seq_node in root
        .descendants()
        .filter(|n| n.is_element() && n.has_tag_name("Sequence"))
    {
        let seq_name = child_text(seq_node, "Name")
            .or_else(|| seq_node.attribute("Name"))
            .unwrap_or("Sequence 01");
        let ticks_per_frame = child_i64(seq_node, "VideoFrameRate")
            .or_else(|| child_i64(seq_node, "FrameRate"))
            .or_else(|| {
                seq_node
                    .attribute("VideoFrameRate")
                    .and_then(|v| v.parse::<i64>().ok())
            })
            .unwrap_or_else(|| FrameRate::FPS_24.frame_duration().0);
        let rate = rate_from_ticks_per_frame(ticks_per_frame);
        let width = child_i64(seq_node, "FrameWidth")
            .or_else(|| child_i64(seq_node, "Width"))
            .or_else(|| {
                seq_node
                    .attribute("Width")
                    .and_then(|v| v.parse::<i64>().ok())
            })
            .unwrap_or(1920)
            .clamp(16, 16384) as u32;
        let height = child_i64(seq_node, "FrameHeight")
            .or_else(|| child_i64(seq_node, "Height"))
            .or_else(|| {
                seq_node
                    .attribute("Height")
                    .and_then(|v| v.parse::<i64>().ok())
            })
            .unwrap_or(1080)
            .clamp(16, 16384) as u32;
        let drop_frame = child_bool(seq_node, "DropFrame").unwrap_or(false);

        let parent_bin = seq_node
            .attribute("Bin")
            .and_then(|bid| bin_map.get(bid).copied());
        let seq_id = b.reserve_sequence(
            seq_name,
            settings_for(rate, width, height, drop_frame),
            parent_bin,
        );
        b.top.push(seq_id);

        let mut seq: Sequence = crate::common::empty_sequence(settings_for(
            rate,
            width,
            height,
            drop_frame,
        ));

        // Sequence-level markers
        for m in elements(seq_node).filter(|n| n.has_tag_name("Marker")) {
            let mid = b.alloc();
            if let Some(marker) = read_marker_node(m, mid) {
                seq.markers.push(marker);
            }
        }
        if let Some(markers_container) = child(seq_node, "Markers") {
            for m in elements(markers_container).filter(|n| n.has_tag_name("Marker")) {
                let mid = b.alloc();
                if let Some(marker) = read_marker_node(m, mid) {
                    seq.markers.push(marker);
                }
            }
        }

        // Video & Audio tracks
        for (tag, kind) in [("VideoTrack", TrackKind::Video), ("AudioTrack", TrackKind::Audio)] {
            let track_nodes: Vec<Node<'_, '_>> = seq_node
                .descendants()
                .filter(|n| n.is_element() && n.has_tag_name(tag))
                .collect();
            for (track_idx, tn) in track_nodes.into_iter().enumerate() {
                b.ensure_tracks(&mut seq, kind, track_idx + 1);
                if let Some(muted) = child_bool(tn, "Muted").or_else(|| {
                    tn.attribute("Muted")
                        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                }) {
                    seq.tracks_mut(kind)[track_idx].muted = muted;
                }
                if let Some(locked) = child_bool(tn, "Locked").or_else(|| {
                    tn.attribute("Locked")
                        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                }) {
                    seq.tracks_mut(kind)[track_idx].locked = locked;
                }

                for item_node in elements(tn) {
                    let tname = item_node.tag_name().name();
                    if matches!(
                        tname,
                        "VideoClipTrackItem" | "AudioClipTrackItem" | "ClipTrackItem" | "ClipItem"
                    ) {
                        let start = child_i64(item_node, "Start")
                            .or_else(|| {
                                item_node
                                    .attribute("Start")
                                    .and_then(|v| v.parse::<i64>().ok())
                            })
                            .unwrap_or(0)
                            .max(0);
                        let end = child_i64(item_node, "End")
                            .or_else(|| {
                                item_node
                                    .attribute("End")
                                    .and_then(|v| v.parse::<i64>().ok())
                            })
                            .unwrap_or(start + rate.frame_duration().0);
                        let duration = (end - start).max(1);
                        let source_in = child_i64(item_node, "InPoint")
                            .or_else(|| child_i64(item_node, "SourceIn"))
                            .or_else(|| {
                                item_node
                                    .attribute("InPoint")
                                    .and_then(|v| v.parse::<i64>().ok())
                            })
                            .unwrap_or(0)
                            .max(0);

                        let media_ref_id = item_node
                            .attribute("MediaRef")
                            .or_else(|| child_text(item_node, "MediaRef"))
                            .unwrap_or("");
                        let clip_path = child_text(item_node, "FilePath")
                            .or_else(|| item_node.attribute("FilePath"))
                            .unwrap_or("");
                        let clip_name = child_text(item_node, "Name")
                            .or_else(|| item_node.attribute("Name"))
                            .unwrap_or("Clip");

                        let media_item = if let Some(&existing) = media_by_id.get(media_ref_id) {
                            existing
                        } else {
                            let effective_path = if clip_path.is_empty() {
                                format!("{clip_name}.mov")
                            } else {
                                resolve_path(clip_path, opts.base_dir.as_deref())
                            };
                            let spec = MediaSpec {
                                duration: Some(Tick(source_in + duration)),
                                video: (kind == TrackKind::Video)
                                    .then_some((width, height, rate)),
                                audio: Some((48_000, 2)),
                                start_tc: None,
                                kind: Some(kind_from_ext(&effective_path)),
                            };
                            let id = b.file_media(
                                &effective_path,
                                clip_name,
                                &effective_path,
                                &spec,
                                None,
                            );
                            if !media_ref_id.is_empty() {
                                media_by_id.insert(media_ref_id.to_owned(), id);
                            }
                            id
                        };

                        let mut ti = b.clip(
                            media_item,
                            kind,
                            clip_name,
                            Tick(start),
                            Tick(duration),
                            Tick(source_in),
                        );

                        if let Some(en) = child_bool(item_node, "Enabled") {
                            ti.enabled = en;
                        }
                        if let Some(spd) = child_f64(item_node, "Speed")
                            && spd.is_finite()
                            && spd > 0.0
                        {
                            ti.speed = spd;
                        }

                        // Clip markers
                        for cm in elements(item_node).filter(|n| n.has_tag_name("Marker")) {
                            let mid = b.alloc();
                            if let Some(mk) = read_marker_node(cm, mid) {
                                ti.markers.push(mk);
                            }
                        }

                        // Clip effects / components
                        for comp in elements(item_node).filter(|n| {
                            matches!(
                                n.tag_name().name(),
                                "Effect"
                                    | "Component"
                                    | "VideoFilterComponent"
                                    | "AudioFilterComponent"
                            )
                        }) {
                            apply_prproj_effect_node(&mut ti, comp, report);
                        }

                        b.place(&mut seq, kind, track_idx, ti);
                    } else if matches!(
                        tname,
                        "TransitionTrackItem"
                            | "VideoTransitionTrackItem"
                            | "AudioTransitionTrackItem"
                    ) {
                        let start = child_i64(item_node, "Start")
                            .or_else(|| {
                                item_node
                                    .attribute("Start")
                                    .and_then(|v| v.parse::<i64>().ok())
                            })
                            .unwrap_or(0)
                            .max(0);
                        let end = child_i64(item_node, "End")
                            .or_else(|| {
                                item_node
                                    .attribute("End")
                                    .and_then(|v| v.parse::<i64>().ok())
                            })
                            .unwrap_or(start + rate.frame_duration().0 * 12);
                        let duration = (end - start).max(1);
                        let cut = start + duration / 2;
                        let name = child_text(item_node, "MatchName")
                            .or_else(|| child_text(item_node, "Name"))
                            .or_else(|| item_node.attribute("MatchName"))
                            .unwrap_or(if kind == TrackKind::Audio {
                                "Constant Power"
                            } else {
                                "Cross Dissolve"
                            });
                        let eff = transition_effect(name, kind == TrackKind::Audio, report);
                        let tid = TransitionId(b.alloc());
                        let track_ref = &mut seq.tracks_mut(kind)[track_idx];
                        let from_clip = track_ref
                            .items
                            .iter()
                            .find(|c| (c.end().0 - cut).abs() <= duration)
                            .map(|c| c.id);
                        let to_clip = track_ref
                            .items
                            .iter()
                            .find(|c| (c.start.0 - cut).abs() <= duration)
                            .map(|c| c.id);
                        track_ref.transitions.push(Transition {
                            id: tid,
                            from: from_clip,
                            to: to_clip,
                            start: Tick(start),
                            duration: Tick(duration),
                            align: TransitionAlign::CenterAtCut,
                            reverse: false,
                            effect: eff,
                        });
                    } else if !matches!(tname, "Muted" | "Locked" | "Name") {
                        report.warn(format!(
                            "unrecognized Premiere track element <{tname}> skipped"
                        ));
                    }
                }
            }
        }

        b.put_sequence(seq_id, seq);
    }

    if b.top.is_empty() {
        return Err(Error::Empty);
    }

    Ok(b.finish())
}

/// Export `sequence` of `project` as a clean-room `<PremiereData>` XML string.
pub fn export(
    project: &Project,
    sequence: ItemId,
    opts: &ExportOptions,
    report: &mut Report,
) -> Result<String> {
    let seq = project.sequence(sequence).ok_or(Error::NoSequence(sequence))?;
    let seq_name = opts
        .name
        .as_deref()
        .or_else(|| project.item(sequence).map(|i| i.name.as_str()))
        .unwrap_or("Sequence");
    let rate = seq.settings.frame_rate;
    let ticks_per_frame = rate.frame_duration().0;

    let mut w = XmlWriter::new(None);
    w.open(
        "PremiereData",
        &[
            ("Version", "3"),
            ("Name", &project.name),
            ("TicksPerSecond", &TICKS_PER_SECOND.to_string()),
        ],
    );

    // Export bins
    for entry in &project.root.children {
        if let BinEntry::Bin(bin) = entry {
            w.open(
                "Bin",
                &[("ObjectID", &bin.id.0.to_string()), ("Name", &bin.name)],
            );
            w.close();
        }
    }

    // Export referenced media items
    for (&id, item) in &project.items {
        if let ItemKind::Media(mc) = &item.kind
            && let Some(path) = item_path(project, id)
        {
            w.open("Media", &[("ObjectID", &id.0.to_string()), ("Name", &item.name)]);
            w.text("FilePath", path);
            w.text("Duration", mc.info.duration.0);
            if let Some(v) = &mc.info.video {
                w.text("Width", v.width);
                w.text("Height", v.height);
                w.text("FrameRate", v.frame_rate.frame_duration().0);
            }
            if let Some(a) = mc.info.audio() {
                w.text("AudioSampleRate", a.sample_rate);
                w.text("AudioChannels", a.channels);
            }
            w.close();
        }
    }

    // Export sequence
    w.open(
        "Sequence",
        &[
            ("ObjectID", &sequence.0.to_string()),
            ("Name", seq_name),
        ],
    );
    w.text("VideoFrameRate", ticks_per_frame);
    w.text("FrameWidth", seq.settings.width);
    w.text("FrameHeight", seq.settings.height);
    w.text("DropFrame", bool_str(seq.settings.drop_frame));

    for m in &seq.markers {
        w.open("Marker", &[]);
        w.text("Name", &m.name);
        w.text("Comment", &m.comment);
        w.text("Start", m.start.0);
        w.text("Duration", m.duration.0);
        w.text(
            "Type",
            match m.kind {
                MarkerKind::Chapter => "Chapter",
                _ => "Comment",
            },
        );
        w.close();
    }

    for (tag, item_tag, trans_tag, tracks) in [
        (
            "VideoTrack",
            "VideoClipTrackItem",
            "VideoTransitionTrackItem",
            &seq.video_tracks,
        ),
        (
            "AudioTrack",
            "AudioClipTrackItem",
            "AudioTransitionTrackItem",
            &seq.audio_tracks,
        ),
    ] {
        for tr in tracks {
            w.open(tag, &[("Name", &tr.name)]);
            w.text("Muted", bool_str(tr.muted));
            w.text("Locked", bool_str(tr.locked));
            for ti in &tr.items {
                if let Some(what) = crate::common::uncarried_clip(project, ti, rate) {
                    report.warn(format!("{what} is not supported in .prproj interchange"));
                    continue;
                }
                let base = crate::common::base_item(project, ti.item);
                w.open(
                    item_tag,
                    &[
                        ("Name", &ti.name),
                        ("MediaRef", &base.0.to_string()),
                    ],
                );
                w.text("Start", ti.start.0);
                w.text("End", ti.end().0);
                w.text("InPoint", ti.source_in.0);
                w.text("Enabled", bool_str(ti.enabled));
                if (ti.speed - 1.0).abs() > 1e-6 {
                    w.text("Speed", num(ti.speed));
                }
                if let Some(p) = item_path(project, base) {
                    w.text("FilePath", p);
                }
                if ti.gain_db.abs() > 1e-6 {
                    w.open("AudioFilterComponent", &[("MatchName", "ADBE Audio Volume")]);
                    w.text("LevelDb", num(ti.gain_db));
                    w.close();
                }
                for eff in &ti.effects {
                    if eff.effect == "lumetri" {
                        w.open("VideoFilterComponent", &[("MatchName", "ADBE Lumetri")]);
                        for (param_id, xml_key) in [
                            ("exposure", "Exposure"),
                            ("contrast", "Contrast"),
                            ("highlights", "Highlights"),
                            ("shadows", "Shadows"),
                            ("whites", "Whites"),
                            ("blacks", "Blacks"),
                            ("temperature", "Temperature"),
                            ("tint", "Tint"),
                            ("saturation", "Saturation"),
                            ("vibrance", "Vibrance"),
                        ] {
                            if let Some(ParamValue::Float(v)) = eff.param(param_id).map(|p| &p.value)
                            {
                                w.text(xml_key, num(*v));
                            }
                        }
                        w.close();
                    }
                }
                for mk in &ti.markers {
                    w.open("Marker", &[]);
                    w.text("Name", &mk.name);
                    w.text("Comment", &mk.comment);
                    w.text("Start", mk.start.0);
                    w.text("Duration", mk.duration.0);
                    w.close();
                }
                w.close();
            }
            for trs in &tr.transitions {
                let cut = trs
                    .from
                    .and_then(|id| tr.item(id))
                    .map(|c| c.end())
                    .or_else(|| trs.to.and_then(|id| tr.item(id)).map(|c| c.start))
                    .unwrap_or(Tick::ZERO);
                let start = (cut.0 - trs.duration.0 / 2).max(0);
                let end = start + trs.duration.0;
                w.open(
                    trans_tag,
                    &[("MatchName", &transition_name(&trs.effect))],
                );
                w.text("Start", start);
                w.text("End", end);
                w.close();
            }
            w.close();
        }
    }

    w.close();
    Ok(w.finish())
}

/// Export `sequence` of `project` as a gzip-compressed `.prproj` byte vector.
pub fn export_gzip(
    project: &Project,
    sequence: ItemId,
    opts: &ExportOptions,
    report: &mut Report,
) -> Result<Vec<u8>> {
    let xml = export(project, sequence, opts, report)?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(xml.as_bytes())
        .map_err(|e| Error::Other(format!("gzip encode failed: {e}")))?;
    encoder
        .finish()
        .map_err(|e| Error::Other(format!("gzip finish failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_and_xml_prproj_roundtrip_preserves_ticks_lumetri_markers_and_reports_unknown_effects() {
        let sample_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PremiereData Version="3" Name="Commercial Spot">
  <Bin ObjectID="bin-1" Name="Rushes"/>
  <Media ObjectID="m-1" Name="Hero.mov" Bin="bin-1">
    <FilePath>/footage/Hero.mov</FilePath>
    <Duration>2540160000000</Duration>
    <Width>1920</Width>
    <Height>1080</Height>
    <FrameRate>10584000000</FrameRate>
  </Media>
  <Sequence ObjectID="seq-1" Name="Main Edit">
    <VideoFrameRate>10584000000</VideoFrameRate>
    <FrameWidth>1920</FrameWidth>
    <FrameHeight>1080</FrameHeight>
    <Marker>
      <Name>Act I Hook</Name>
      <Comment>Opening beat</Comment>
      <Start>0</Start>
      <Duration>0</Duration>
    </Marker>
    <VideoTrack Name="Video 1">
      <VideoClipTrackItem Name="Hero Shot A" MediaRef="m-1">
        <Start>0</Start>
        <End>1270080000000</End>
        <InPoint>254016000000</InPoint>
        <VideoFilterComponent MatchName="ADBE Lumetri">
          <Contrast>18.5</Contrast>
          <Temperature>4.0</Temperature>
        </VideoFilterComponent>
        <VideoFilterComponent MatchName="ADBE Warp Stabilizer">
          <Smoothness>50</Smoothness>
        </VideoFilterComponent>
      </VideoClipTrackItem>
      <VideoClipTrackItem Name="Hero Shot B" MediaRef="m-1">
        <Start>1270080000000</Start>
        <End>2540160000000</End>
        <InPoint>0</InPoint>
      </VideoClipTrackItem>
      <VideoTransitionTrackItem MatchName="Cross Dissolve">
        <Start>1206576000000</Start>
        <End>1333584000000</End>
      </VideoTransitionTrackItem>
    </VideoTrack>
    <AudioTrack Name="Audio 1">
      <AudioClipTrackItem Name="Hero Audio" MediaRef="m-1">
        <Start>0</Start>
        <End>2540160000000</End>
        <InPoint>0</InPoint>
        <AudioFilterComponent MatchName="ADBE Audio Volume">
          <LevelDb>-3.5</LevelDb>
        </AudioFilterComponent>
      </AudioClipTrackItem>
    </AudioTrack>
  </Sequence>
</PremiereData>"#;

        // Compress with gzip to test genuine .prproj binary container
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(sample_xml.as_bytes()).unwrap();
        let gz_bytes = gz.finish().unwrap();

        assert!(sniff(&gz_bytes, Some("prproj")));
        assert!(sniff(sample_xml.as_bytes(), None));

        let mut report = Report::default();
        let imported = import_bytes(&gz_bytes, &ImportOptions::default(), &mut report).unwrap();
        assert_eq!(imported.sequences.len(), 1);
        assert!(report.mentions("ADBE Warp Stabilizer"));

        let seq_id = imported.sequences[0];
        let seq = imported.project.sequence(seq_id).unwrap();
        assert_eq!(seq.settings.frame_rate, FrameRate::FPS_24);
        assert_eq!(seq.markers.len(), 1);
        assert_eq!(seq.video_tracks[0].items.len(), 2);
        assert_eq!(seq.video_tracks[0].transitions.len(), 1);
        assert_eq!(seq.audio_tracks[0].items[0].gain_db, -3.5);

        // Round-trip export to gzipped .prproj and re-import
        let mut exp_report = Report::default();
        let exported_gz =
            export_gzip(&imported.project, seq_id, &ExportOptions::default(), &mut exp_report)
                .unwrap();
        let mut reimport_report = Report::default();
        let reimported =
            import_bytes(&exported_gz, &ImportOptions::default(), &mut reimport_report).unwrap();
        let re_seq = reimported.project.sequence(reimported.sequences[0]).unwrap();
        assert_eq!(re_seq.video_tracks[0].items.len(), 2);
        assert_eq!(re_seq.audio_tracks[0].items[0].gain_db, -3.5);
    }
}
